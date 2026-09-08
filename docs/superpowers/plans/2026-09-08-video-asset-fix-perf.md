# 视频 asset 访问修复 + remux 缓存性能 实现计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 修复视频 `<video>` asset 加载失败(code 4，产物在系统临时目录被 asset 拒)，把 remux 产物改放应用数据目录并做缓存复用 + LRU 上限清理，使打开(秒开/复用)、播放、拖进度性能极致。

**Architecture:** 在现有 remux 方案上三处改动：remux 产物目录从 `std::env::temp_dir()` 改到 `<app_data_dir>/video_cache/`(asset 可访问)；加 LRU 缓存上限清理；player_open 取 app_data 传入，player_stop 不再删产物(改由 LRU 管)。视频 H264 免重编码只转音频 AAC，本地 mp4 + moov faststart 已是最优播放/seek。

**Tech Stack:** Rust(std::process ffmpeg/ffprobe、std::fs)、Tauri 2.11 app_data_dir + asset 协议、前端 `<video>` + convertFileSrc。

**参考设计：** `docs/superpowers/specs/2026-09-08-video-asset-fix-perf-design.md`

**根因(已确证)：** dev 日志 `video error 4 src=asset://localhost/%2Fvar%2Ffolders%2F...T%2Fcollector_xxx.mp4` — 产物在 macOS 系统临时目录 `/var/folders`，asset 协议不放行；而封面图在素材目录能被 asset 加载。

**当前代码(transcode.rs)：** `probe_duration(path)->f64`、`temp_mp4_path(src)->PathBuf`(用 `std::env::temp_dir()`)、`remux(path)->(String,f64)`(复用+ `-c:v copy -c:a aac` + faststart)。`player/mod.rs`：PlayerState(Mutex<Option<String>>)、player_open(state,path)->PlayerInfo{src,duration}、player_stop(state)删产物。

---

## 文件结构
- `src-tauri/src/player/transcode.rs` — cache_dir、mp4 路径改缓存目录、remux 加 cache_dir 参数、LRU enforce
- `src-tauri/src/player/mod.rs` — player_open 取 app_data 传 remux；player_stop 不删缓存
- `src-tauri/src/lib.rs` — player_open command 加 AppHandle（若需）
- `src/views/PlayerView.ts` — 无需改（验证用）

---

## Task 1: remux 产物改放应用缓存目录

**Files:** Modify `src-tauri/src/player/transcode.rs`

- [ ] **Step 1: 改路径函数 + remux 加 cache_dir 参数 + 更新测试**

把 `temp_mp4_path` 替换为基于传入缓存目录的版本，`remux` 加 `cache_dir: &Path` 参数：
```rust
use std::path::{Path, PathBuf};
// （删掉 use std::env 不再需要；保留 hash 相关 use）

/// 缓存目录下某源视频对应的 mp4 产物路径（路径 hash 命名，稳定可复用）。
pub fn cached_mp4_path(cache_dir: &Path, src: &str) -> PathBuf {
    let mut h = DefaultHasher::new();
    src.hash(&mut h);
    cache_dir.join(format!("collector_{:016x}.mp4", h.finish()))
}

/// remux 到缓存目录。cache_dir 由调用方传入（应用数据目录下的 video_cache）。
pub fn remux(cache_dir: &Path, path: &str) -> AppResult<(String, f64)> {
    std::fs::create_dir_all(cache_dir).ok();
    let duration = probe_duration(path)?;
    let out = cached_mp4_path(cache_dir, path);
    if out.metadata().map(|m| m.len() > 0).unwrap_or(false) {
        // 复用：touch mtime 供 LRU 识别"最近用过"
        let _ = filetime_touch(&out);
        return Ok((out.to_string_lossy().into_owned(), duration));
    }
    let status = Command::new("ffmpeg")
        .args([
            "-nostdin", "-i", path,
            "-c:v", "copy", "-c:a", "aac", "-b:a", "192k",
            "-movflags", "+faststart", "-y",
            &out.to_string_lossy(),
        ])
        .status()
        .map_err(|e| AppError::Other(format!("ffmpeg spawn: {e}")))?;
    if !status.success() {
        let _ = std::fs::remove_file(&out);
        return Err(AppError::Other(
            "无法转封装该视频（可能编码不受支持，当前仅支持 H.264）".into(),
        ));
    }
    Ok((out.to_string_lossy().into_owned(), duration))
}

/// 更新文件 mtime 为当前时间（LRU 用；无 filetime crate 时用重写方式）。
/// 用 std：打开文件后设置修改时间。std 无直接 set_mtime，改用 File::open + 无操作
/// 不改 mtime，故这里用一个轻量实现：读取后写回 0 字节不可行。
/// 简化：复用时不 touch，LRU 用文件创建时间近似——见 Task 2 用 mtime 排序即可，
/// 复用旧文件天然 mtime 旧、会被优先淘汰。为让"常看的保留"，touch 更好：
/// 用 `utime` via std::fs 不支持，故引入 filetime crate（Task 2 Step 0 加依赖）。
fn filetime_touch(p: &Path) -> std::io::Result<()> {
    let now = filetime::FileTime::now();
    filetime::set_file_mtime(p, now)
}
```
注：`filetime` crate 在 Task 2 Step 0 加依赖；本 Step 先写调用，Task 2 加依赖后一起编译。为让本 Task 可独立编译，**本 Step 先不写 filetime_touch，复用分支不 touch**（改为直接 return），touch 留到 Task 2 与 LRU 一起做。即本 Step 的复用分支简化为：
```rust
    if out.metadata().map(|m| m.len() > 0).unwrap_or(false) {
        return Ok((out.to_string_lossy().into_owned(), duration));
    }
```
（不引入 filetime_touch，Task 2 再加 touch。）

更新测试（temp_path_is_stable_and_mp4 → cached path 版）：
```rust
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cached_path_is_stable_and_mp4() {
        let dir = Path::new("/cache");
        let a = cached_mp4_path(dir, "/movies/x.mkv");
        let b = cached_mp4_path(dir, "/movies/x.mkv");
        assert_eq!(a, b);
        assert!(a.starts_with("/cache"));
        assert!(a.to_string_lossy().ends_with(".mp4"));
        assert_ne!(a, cached_mp4_path(dir, "/movies/y.mkv"));
    }
}
```

- [ ] **Step 2: 编译（会因 mod.rs 调用旧 remux 签名报错，Task 3 修）**

Run: `cd src-tauri && cargo build 2>&1 | tail -6`
Expected: transcode.rs 本身编译，但 mod.rs 调 `remux(&path)`（旧签名）会报参数不匹配——**本 Task 与 Task 3 一起提交**（同一签名变更）。或先跳过 build 到 Task 3。为可独立验证，本 Task 先只跑 transcode 的单测：`cargo test player::transcode::tests::cached_path_is_stable_and_mp4`（测试不依赖 mod.rs）。
Expected: 该单测 PASS。

- [ ] **Step 3: 不单独提交，与 Task 2、3 一起（签名变更连锁）**

## Task 2: LRU 缓存上限清理

**Files:** Modify `src-tauri/Cargo.toml`, `src-tauri/src/player/transcode.rs`

- [ ] **Step 0: 加 filetime 依赖**

`src-tauri/Cargo.toml` `[dependencies]` 加：`filetime = "0.2"`

- [ ] **Step 1: 加 LRU enforce + 复用时 touch + 测试**

在 transcode.rs 加：
```rust
use std::path::Path;

/// 缓存上限（字节）。默认 20 GiB。
pub const CACHE_LIMIT_BYTES: u64 = 20 * 1024 * 1024 * 1024;

/// 更新 mtime 为现在（复用时调用，让常看的视频在 LRU 中"更新鲜"）。
pub fn touch(p: &Path) {
    let _ = filetime::set_file_mtime(p, filetime::FileTime::now());
}

/// 强制缓存目录总大小不超过 max_bytes：超出则按 mtime 从最旧删起，直到达标。
/// 只处理 collector_*.mp4。
pub fn enforce_cache_limit(cache_dir: &Path, max_bytes: u64) {
    let mut files: Vec<(PathBuf, u64, filetime::FileTime)> = Vec::new();
    let rd = match std::fs::read_dir(cache_dir) { Ok(r) => r, Err(_) => return };
    for e in rd.filter_map(|e| e.ok()) {
        let p = e.path();
        let name = p.file_name().and_then(|s| s.to_str()).unwrap_or("");
        if !(name.starts_with("collector_") && name.ends_with(".mp4")) { continue; }
        if let Ok(m) = e.metadata() {
            files.push((p.clone(), m.len(), filetime::FileTime::from_last_modification_time(&m)));
        }
    }
    let total: u64 = files.iter().map(|(_, s, _)| *s).sum();
    if total <= max_bytes { return; }
    // 按 mtime 升序（最旧在前），从最旧删起直到达标
    files.sort_by_key(|(_, _, t)| *t);
    let mut cur = total;
    for (p, size, _) in files {
        if cur <= max_bytes { break; }
        if std::fs::remove_file(&p).is_ok() { cur -= size; }
    }
}
```
并在 `remux` 的复用分支加 `touch(&out);`，在 remux 成功产出后加 `enforce_cache_limit(cache_dir, CACHE_LIMIT_BYTES);`：
```rust
    if out.metadata().map(|m| m.len() > 0).unwrap_or(false) {
        touch(&out);
        return Ok((out.to_string_lossy().into_owned(), duration));
    }
    // ... ffmpeg remux ...
    // 成功后：
    enforce_cache_limit(cache_dir, CACHE_LIMIT_BYTES);
    Ok((out.to_string_lossy().into_owned(), duration))
```

测试：
```rust
    #[test]
    fn lru_deletes_oldest_over_limit() {
        use std::io::Write;
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path();
        // 造 3 个 collector_*.mp4，各 1000 字节，mtime 递增
        for (i, name) in ["collector_a.mp4","collector_b.mp4","collector_c.mp4"].iter().enumerate() {
            let p = dir.join(name);
            let mut f = std::fs::File::create(&p).unwrap();
            f.write_all(&vec![0u8; 1000]).unwrap();
            let t = filetime::FileTime::from_unix_time(1000 + i as i64, 0);
            filetime::set_file_mtime(&p, t).unwrap();
        }
        // 上限 2500 → 需删到 <=2500，最旧的 a 被删（3000→2000）
        enforce_cache_limit(dir, 2500);
        assert!(!dir.join("collector_a.mp4").exists());
        assert!(dir.join("collector_b.mp4").exists());
        assert!(dir.join("collector_c.mp4").exists());
    }
```

- [ ] **Step 2: 运行测试** — `cd src-tauri && cargo test player::transcode`。Expected: cached_path + lru 两测试 PASS（编译仍需 Task 3 修 mod.rs 调用，若 mod.rs 报错先做 Task 3 再统一 test）。

## Task 3: player_open 取 app_data + player_stop 不删缓存

**Files:** Modify `src-tauri/src/player/mod.rs`, `src-tauri/src/lib.rs`

- [ ] **Step 1: 改 player_open 取 app_data 传 remux；player_stop 不删缓存**

`src-tauri/src/player/mod.rs` 替换 player_open/player_stop：
```rust
use crate::error::AppResult;
use serde::Serialize;
use std::sync::Mutex;
use tauri::Manager;

#[derive(Default)]
pub struct PlayerState(pub Mutex<Option<String>>);

#[derive(Serialize)]
pub struct PlayerInfo {
    pub src: String,
    pub duration: f64,
}

/// 打开视频：remux 到应用缓存目录（asset 可访问），返回 {src, duration}。
/// 缓存复用 + LRU 由 transcode 管理，播放器只记当前产物路径。
#[tauri::command]
pub fn player_open(app: tauri::AppHandle, state: tauri::State<PlayerState>, path: String) -> AppResult<PlayerInfo> {
    let cache_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| crate::error::AppError::Other(format!("app_data_dir: {e}")))?
        .join("video_cache");
    let (src, duration) = transcode::remux(&cache_dir, &path)?;
    *state.0.lock().unwrap() = Some(src.clone());
    Ok(PlayerInfo { src, duration })
}

/// 停止播放：只清状态，**不删缓存产物**（缓存复用，磁盘由 LRU 管理）。
#[tauri::command]
pub fn player_stop(state: tauri::State<PlayerState>) -> AppResult<()> {
    *state.0.lock().unwrap() = None;
    Ok(())
}
```
（顶部保留 `pub mod transcode;`。）

- [ ] **Step 2: lib.rs 确认 player_open 注册无变（command 加了 AppHandle 参数，Tauri 自动注入，generate_handler 里仍是 player::player_open，无需改注册）**

确认 `src-tauri/src/lib.rs` 的 generate_handler! 里有 `player::player_open, player::player_stop`（已在，无需改）。

- [ ] **Step 3: 编译 + 测试**

Run: `cd src-tauri && cargo build 2>&1 | tail -5` — Expected 通过。
Run: `cd src-tauri && cargo test player::transcode` — Expected cached_path + lru 测试 PASS。
Run: `npm run build 2>&1 | tail -3` — Expected 前端通过（PlayerView 未改，playerOpen 返回形状不变）。

- [ ] **Step 4: 提交（Task 1+2+3 一起）**

```bash
git add -A && git commit -m "fix: remux to app data cache dir (asset-accessible) + LRU eviction"
```

## Task 4: 真机验证

**Files:** 无（验证）

- [ ] **Step 1: 重启应用**

```bash
pkill -f "tauri dev"; pkill -f "target/debug/collector"; pkill ffmpeg; sleep 1
cd /Users/zhoumo/Documents/Claude/collector && nohup npm run tauri dev > /tmp/collector-dev.log 2>&1 & disown
```

- [ ] **Step 2: 验证清单**

1. 点视频 → "准备中…转封装" 几秒 → `<video>` **出画面 + 有声音**（asset 从 app_data/video_cache 加载成功，修好 code 4）。
2. 同一视频再开 → 秒开（缓存复用，touch 更新 mtime）。
3. 拖进度 → 丝滑无延迟（本地 mp4 + moov faststart 原生 seek）。
4. 快进快退/音量/全屏正常。
5. 退出 → 立即切回、不崩、缓存保留（`ls <app_data>/video_cache/` 有产物）。
6. 播放多个视频累计超 20GB → 最旧缓存被自动删（可临时把 CACHE_LIMIT_BYTES 调小测，验证后改回）。

- [ ] **Step 3: 若仍 code 4（app_data 也被 asset 拒）**

真机验证的唯一关键风险点。若 `<video>` 仍报 code 4：
- 读 dev 日志确认 asset URL 里的路径（是否 app_data 路径）。
- 检查 `src-tauri/capabilities/default.json` 是否需加 asset/fs 相关权限；或 tauri.conf.json 的 assetProtocol scope 显式加 app_data 路径（如 `["**", "$APPDATA/**"]`）。
- 调整后重启再验。把结论记录。

- [ ] **Step 4: 记录验证结果**（无代码改动则不提交）

---

## 自查

**1. Spec 覆盖：**
- 产物改放应用数据目录（修 code 4）→ Task 1 cached_mp4_path + Task 3 player_open 取 app_data ✓
- remux 缓存复用 → Task 1 复用分支 + Task 2 touch ✓
- LRU 上限自动清 → Task 2 enforce_cache_limit ✓
- player_stop 不删缓存 → Task 3 ✓
- 播放/拖进度性能（asset 本地 mp4 + faststart 原生）→ 现状已满足，无需改（Task 4 验证）✓
- 真机验证 asset 访问 → Task 4 ✓

**2. 占位符扫描：** 无 TBD。Task 1 Step 1 里关于 filetime_touch 的说明是"本 Task 先简化不 touch、Task 2 加"的明确执行顺序说明，非占位；Task 4 Step 3 是条件性 fallback（真机风险应对），有明确判断步骤。

**3. 类型一致性：** `cached_mp4_path(cache_dir,src)`、`remux(cache_dir,path)`、`enforce_cache_limit(cache_dir,max)`、`touch(p)`、`CACHE_LIMIT_BYTES` 在 Task 1/2 定义、Task 3 用 remux 新签名。player_open 加 `app: AppHandle` 参数（Tauri 自动注入，generate_handler 注册名不变）。PlayerInfo{src,duration} 形状不变，前端 playerOpen 无需改。

自查通过。
