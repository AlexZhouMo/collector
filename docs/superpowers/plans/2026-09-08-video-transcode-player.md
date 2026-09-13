# 视频转码 + DOM `<video>` 播放器 实现计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 把视频播放从 libmpv 嵌入（macOS 交通灯/错位/seek消失等无解 bug）彻底换成 ffmpeg 按需转码 + DOM `<video>` 内嵌，从架构上根除所有窗口/坐标问题。

**Architecture:** 后端用系统 ffmpeg 按需转码（H.264 直接 remux；HEVC 用 VideoToolbox 硬编转 H.264，实测 4K 2.5x 实时）输出 fMP4 到 stdout；经 Tauri 自定义协议 `stream://` 流式喂给前端 `<video>`。seek 用 `-ss` 重起转码。视频是普通 DOM 元素，天然内嵌/跟随布局/无交通灯。

**Tech Stack:** 系统 ffmpeg/ffprobe CLI（std::process），Tauri 2.x 自定义协议（register_asynchronous_uri_scheme_protocol），前端 HTML `<video>`。

**参考设计：** `docs/superpowers/specs/2026-09-08-video-transcode-player-design.md`

**关键已验证事实：**
- 4K HEVC→4K H.264 硬编（h264_videotoolbox）实测 **2.53x 实时**（fps 61），足够流畅，方案可行。
- H.264 源直接 `-c:v copy` remux 瞬时。
- ffmpeg fMP4 流式：`-movflags frag_keyframe+empty_moov+default_base_moof -f mp4 pipe:1`。
- 开发机 `/opt/homebrew/bin/ffmpeg`+`ffprobe` 可用。

**当前要移除的 mpv 代码：** `player/mpv.rs`、`player/embed_macos.rs`、libmpv2/objc2 依赖、`player_embed/player_set_bounds/player_close_window/player_load/player_pause/player_seek/player_seek_to/player_volume/player_progress/player_fullscreen` command、PlayerView 的 mpv 逻辑、tauri.conf 的 macOSPrivateApi。

---

## 文件结构

- `src-tauri/src/player/transcode.rs` — 新增：ffprobe 探测 + ffmpeg 转码会话（起/停/seek 重起，进程管理）
- `src-tauri/src/player/mod.rs` — 改造：PlayerState 改为持有转码会话；player_open/player_seek/player_stop/player_duration command
- `src-tauri/src/lib.rs` — 注册 `stream://` 自定义协议；替换 command 列表；移除 mpv
- `src-tauri/Cargo.toml` — 移除 libmpv2 + objc2*
- `src-tauri/tauri.conf.json` — 移除 macOSPrivateApi；`.cargo/config.toml` 移除 libmpv 链接路径
- `src/views/PlayerView.ts` — 改为 `<video>` + 玻璃控制条
- `src/lib/ipc.ts` — player command 调整

---

## Task 1: 移除 mpv 播放子系统

**Files:** Delete `src-tauri/src/player/mpv.rs`, `src-tauri/src/player/embed_macos.rs`；Modify `src-tauri/src/player/mod.rs`, `src-tauri/src/lib.rs`, `src-tauri/Cargo.toml`, `src-tauri/tauri.conf.json`, `src-tauri/.cargo/config.toml`

- [ ] **Step 1: 删除 mpv 文件与依赖**

```bash
cd /Users/zhoumo/Documents/Claude/collector/src-tauri
rm src/player/mpv.rs src/player/embed_macos.rs
```
`Cargo.toml`：删除 `libmpv2 = "6.0.0"` 行；删除整个 `[target.'cfg(target_os = "macos")'.dependencies]` 段（objc2/objc2-app-kit/objc2-foundation）；把 `tauri` 的 features 从 `["protocol-asset", "macos-private-api"]` 改回 `["protocol-asset"]`。
`tauri.conf.json`：删除 `"macOSPrivateApi": true,` 行。
`.cargo/config.toml`：删除全部内容（libmpv 链接路径不再需要）或删除该文件。

- [ ] **Step 2: 清空 player/mod.rs 为占位**

把 `src-tauri/src/player/mod.rs` 整个替换为（先清空 mpv 相关，Task 2/3 再填真实内容）：
```rust
//! 视频播放：ffmpeg 按需转码 + 前端 <video> 播放。
pub mod transcode;

use std::sync::Mutex;

/// 当前转码会话（同一时刻只播一个视频）。Task 2 定义 TranscodeSession。
#[derive(Default)]
pub struct PlayerState(pub Mutex<Option<transcode::TranscodeSession>>);
```
创建占位 `src-tauri/src/player/transcode.rs`：
```rust
//! ffmpeg 转码会话（Task 2 实现）。
pub struct TranscodeSession;
```

- [ ] **Step 3: lib.rs 移除 mpv command，暂时清空 player 注册**

`src-tauri/src/lib.rs`：
- 删除 `player_embed`/`player_set_bounds`/`player_close_window`/`player_fullscreen` 这几个定义在 lib.rs 里的 command 函数。
- 从 `generate_handler!` 移除所有 `player::player_*` 和上述 command（Task 3 会加回新的）。
- `setup` 里 `app.manage(player::PlayerState::default())` 保留；`app.manage(player::EmbedState::default())` 删除（EmbedState 已不存在）。
- 保留 `set_video_pos`/`get_video_pos`（续播，复用）。

- [ ] **Step 4: 编译**

Run: `cd src-tauri && cargo build 2>&1 | tail -8`
Expected: 编译通过（player 暂无 command，前端 PlayerView 还引用旧 ipc 但 Rust 侧不管前端；若前端 build 报错本任务先不管，Task 4/5 修）。可能有 unused 警告，可接受。

- [ ] **Step 5: 提交**

```bash
git add -A && git commit -m "chore: remove libmpv/objc2 player subsystem"
```

## Task 2: ffmpeg 转码会话（transcode.rs）

**Files:** Modify `src-tauri/src/player/transcode.rs`

- [ ] **Step 1: 写 ffprobe 探测 + 会话结构 + 测试**

把 `src-tauri/src/player/transcode.rs` 替换为：
```rust
//! ffmpeg 按需转码会话：探测源编码，起 ffmpeg 输出 fMP4 到 stdout，
//! 供自定义协议流式读取。H.264 直接 remux，其它编码 VideoToolbox 硬编转 H.264。
use crate::error::{AppError, AppResult};
use std::io::Read;
use std::process::{Child, Command, Stdio};

/// 源视频探测结果。
#[derive(Debug, Clone)]
pub struct Probe {
    pub video_is_h264: bool,
    pub audio_is_aac: bool,
    pub duration_secs: f64,
}

/// 用 ffprobe 探测视频/音频编码与时长。
pub fn probe(path: &str) -> AppResult<Probe> {
    let out = Command::new("ffprobe")
        .args([
            "-v", "error",
            "-show_entries", "stream=codec_type,codec_name",
            "-show_entries", "format=duration",
            "-of", "default=noprint_wrappers=1",
            path,
        ])
        .output()
        .map_err(|e| AppError::Other(format!("ffprobe spawn: {e}")))?;
    let text = String::from_utf8_lossy(&out.stdout);
    // 简单解析：逐行找 codec_type/codec_name 配对，及 duration
    let mut video_is_h264 = false;
    let mut audio_is_aac = false;
    let mut duration_secs = 0.0;
    let mut cur_type = String::new();
    for line in text.lines() {
        if let Some(v) = line.strip_prefix("codec_type=") {
            cur_type = v.trim().to_string();
        } else if let Some(v) = line.strip_prefix("codec_name=") {
            let name = v.trim();
            if cur_type == "video" && name == "h264" {
                video_is_h264 = true;
            }
            if cur_type == "audio" && name == "aac" {
                audio_is_aac = true;
            }
        } else if let Some(v) = line.strip_prefix("duration=") {
            duration_secs = v.trim().parse().unwrap_or(0.0);
        }
    }
    Ok(Probe { video_is_h264, audio_is_aac, duration_secs })
}

/// 构造 ffmpeg 参数：按需选择 copy / videotoolbox 硬编。
/// start_secs > 0 时从该位置起转（seek）。输出 fMP4 到 stdout(pipe:1)。
pub fn build_args(path: &str, probe: &Probe, start_secs: f64) -> Vec<String> {
    let mut args: Vec<String> = Vec::new();
    args.push("-nostdin".into());
    if start_secs > 0.0 {
        args.push("-ss".into());
        args.push(format!("{start_secs}"));
    }
    args.push("-i".into());
    args.push(path.into());
    // 视频：H.264 直接 copy，否则 VideoToolbox 硬编
    args.push("-c:v".into());
    if probe.video_is_h264 {
        args.push("copy".into());
    } else {
        args.push("h264_videotoolbox".into());
        args.push("-b:v".into());
        args.push("20M".into());
    }
    // 音频：AAC copy，否则转 aac
    args.push("-c:a".into());
    if probe.audio_is_aac {
        args.push("copy".into());
    } else {
        args.push("aac".into());
    }
    // fMP4 流式输出到 stdout
    args.push("-movflags".into());
    args.push("frag_keyframe+empty_moov+default_base_moof".into());
    args.push("-f".into());
    args.push("mp4".into());
    args.push("pipe:1".into());
    args
}

/// 一个转码会话：持有 ffmpeg 子进程，其 stdout 是 fMP4 流。
pub struct TranscodeSession {
    child: Child,
    pub path: String,
    pub duration_secs: f64,
}

impl TranscodeSession {
    /// 起一个转码会话。start_secs 为起始位置（seek 用）。
    pub fn start(path: &str, start_secs: f64) -> AppResult<Self> {
        let probe = probe(path)?;
        let args = build_args(path, &probe, start_secs);
        let child = Command::new("ffmpeg")
            .args(&args)
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .stdin(Stdio::null())
            .spawn()
            .map_err(|e| AppError::Other(format!("ffmpeg spawn: {e}")))?;
        Ok(TranscodeSession {
            child,
            path: path.to_string(),
            duration_secs: probe.duration_secs,
        })
    }

    /// 从 ffmpeg stdout 读一块数据。返回读到的字节数（0 = 流结束）。
    pub fn read_chunk(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        match self.child.stdout.as_mut() {
            Some(out) => out.read(buf),
            None => Ok(0),
        }
    }

    /// 停止会话：kill ffmpeg 子进程（退出/seek 时调用，瞬时，不阻塞）。
    pub fn stop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

impl Drop for TranscodeSession {
    fn drop(&mut self) {
        self.stop();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn build_args_h264_uses_copy() {
        let p = Probe { video_is_h264: true, audio_is_aac: true, duration_secs: 100.0 };
        let args = build_args("/x.mkv", &p, 0.0);
        let joined = args.join(" ");
        assert!(joined.contains("-c:v copy"));
        assert!(joined.contains("-c:a copy"));
        assert!(joined.contains("pipe:1"));
    }
    #[test]
    fn build_args_hevc_uses_videotoolbox_and_seek() {
        let p = Probe { video_is_h264: false, audio_is_aac: false, duration_secs: 100.0 };
        let args = build_args("/x.mkv", &p, 42.0);
        let joined = args.join(" ");
        assert!(joined.contains("h264_videotoolbox"));
        assert!(joined.contains("aac"));
        assert!(joined.contains("-ss 42"));
    }
}
```

- [ ] **Step 2: 运行测试**

Run: `cd src-tauri && cargo test player::transcode`
Expected: 2 tests PASS。

- [ ] **Step 3: 编译**

Run: `cd src-tauri && cargo build 2>&1 | tail -3`
Expected: 编译通过。

- [ ] **Step 4: 提交**

```bash
git add -A && git commit -m "feat: ffmpeg transcode session (probe + on-demand args)"
```

## Task 3: 自定义协议 stream:// + player command

**Files:** Modify `src-tauri/src/player/mod.rs`, `src-tauri/src/lib.rs`

- [ ] **Step 1: player/mod.rs 加 command**

把 `src-tauri/src/player/mod.rs` 替换为：
```rust
//! 视频播放：ffmpeg 按需转码 + 前端 <video> 播放。
pub mod transcode;

use crate::error::AppResult;
use std::sync::Mutex;
use transcode::TranscodeSession;

/// 当前转码会话（同一时刻只播一个视频）。
#[derive(Default)]
pub struct PlayerState(pub Mutex<Option<TranscodeSession>>);

/// 打开视频：起一个从头开始的转码会话，返回时长（秒，供前端进度条）。
#[tauri::command]
pub fn player_open(state: tauri::State<PlayerState>, path: String) -> AppResult<f64> {
    let session = TranscodeSession::start(&path, 0.0)?;
    let dur = session.duration_secs;
    let mut guard = state.0.lock().unwrap();
    if let Some(old) = guard.take() {
        drop(old); // 显式停旧会话
    }
    *guard = Some(session);
    Ok(dur)
}

/// seek：kill 旧会话，从 secs 起重开转码会话。前端随后重载 <video>。
#[tauri::command(rename_all = "camelCase")]
pub fn player_seek(state: tauri::State<PlayerState>, path: String, secs: f64) -> AppResult<()> {
    let session = TranscodeSession::start(&path, secs)?;
    let mut guard = state.0.lock().unwrap();
    if let Some(old) = guard.take() {
        drop(old);
    }
    *guard = Some(session);
    Ok(())
}

/// 停止播放：kill 会话（退出播放模式时调用）。
#[tauri::command]
pub fn player_stop(state: tauri::State<PlayerState>) -> AppResult<()> {
    let mut guard = state.0.lock().unwrap();
    if let Some(old) = guard.take() {
        drop(old);
    }
    Ok(())
}
```

- [ ] **Step 2: lib.rs 注册 stream:// 协议 + command**

在 `src-tauri/src/lib.rs` 的 `tauri::Builder::default()` 链上（`.plugin(...)` 附近）加自定义协议注册：
```rust
        .register_asynchronous_uri_scheme_protocol("stream", |ctx, _request, responder| {
            // 从当前转码会话的 ffmpeg stdout 持续读取，分块流式返回。
            let app = ctx.app_handle().clone();
            std::thread::spawn(move || {
                use tauri::Manager;
                let state = app.state::<player::PlayerState>();
                // 简化流式：循环读 stdout 直到结束，聚合后一次性响应。
                // （fMP4 empty_moov 头可渐进解码；大文件建议后续改真流式 body。）
                let mut data = Vec::new();
                {
                    let mut guard = state.0.lock().unwrap();
                    if let Some(session) = guard.as_mut() {
                        let mut buf = [0u8; 65536];
                        loop {
                            match session.read_chunk(&mut buf) {
                                Ok(0) => break,
                                Ok(n) => data.extend_from_slice(&buf[..n]),
                                Err(_) => break,
                            }
                        }
                    }
                }
                responder.respond(
                    tauri::http::Response::builder()
                        .status(200)
                        .header("Content-Type", "video/mp4")
                        .header("Access-Control-Allow-Origin", "*")
                        .body(data)
                        .unwrap(),
                );
            });
        })
```
在 `generate_handler!` 加 `player::player_open, player::player_seek, player::player_stop`（保留 set_video_pos/get_video_pos）。

**注意**：上面是"读完再响应"的简化版（先跑通）。真流式（边转边喂）需要用 responder 的分块/channel body——Task 6 真机验证时若首帧等待过久再优化为流式。先用简化版验证端到端通路。

- [ ] **Step 3: 编译**

Run: `cd src-tauri && cargo build 2>&1 | tail -5`
Expected: 编译通过。若 `register_asynchronous_uri_scheme_protocol` 签名/responder API 与此不符，以 Tauri 2.11 实际 API 为准调整（查 tauri::Builder 的协议注册方法），保证编译通过。

- [ ] **Step 4: 提交**

```bash
git add -A && git commit -m "feat: stream:// protocol and player open/seek/stop commands"
```

## Task 4: 前端 IPC 调整

**Files:** Modify `src/lib/ipc.ts`

- [ ] **Step 1: 替换 player 相关 ipc**

删除 `src/lib/ipc.ts` 里旧的 `playerEmbed/playerSetBounds/playerStop/playerClose/playerCloseWindow/playerLoad/playerPause/playerSeek/playerSeekTo/playerVolume/playerProgress/playerFullscreen` 等（凡对应已删后端 command 的），替换为：
```ts
  playerOpen: (path: string) => invoke<number>("player_open", { path }),
  playerSeek: (path: string, secs: number) => invoke<void>("player_seek", { path, secs }),
  playerStop: () => invoke<void>("player_stop"),
```
保留 `setVideoPos`/`getVideoPos`。

- [ ] **Step 2: 编译**

Run: `npm run build 2>&1 | tail -8`
Expected: tsc 报 PlayerView.ts 引用了已删的 ipc 方法（下个任务重写）。本任务与 Task 5 一起提交（同一契约变更）。跳到 Task 5。

## Task 5: 前端 PlayerView 改为 `<video>`

**Files:** Modify `src/views/PlayerView.ts`

- [ ] **Step 1: 整体重写 PlayerView.ts**

替换为：
```ts
import { api } from "../lib/ipc";
import type { MediaItem } from "../lib/ipc";
import { icon } from "../lib/icons";
import { esc } from "../lib/escape";

export async function PlayerView(it: MediaItem, onExit: () => void): Promise<HTMLElement> {
  const el = document.createElement("div");
  el.className = "player-view view-enter";
  el.innerHTML = `
    <div class="player-top">
      <button class="back icon-text">${icon("arrowLeft", 16)}<span>返回</span></button>
      <span class="player-title">${esc(it.title)}</span>
    </div>
    <div class="player-stage-wrap">
      <video class="player-video" playsinline></video>
    </div>
    <div class="player-bar glass">
      <button class="pp">${icon("pause", 18)}</button>
      <button class="rw">${icon("rewind", 18)}</button>
      <button class="ff">${icon("forward", 18)}</button>
      <input class="seek" type="range" min="0" max="1000" value="0"/>
      <span class="time">0:00 / 0:00</span>
      <input class="vol" type="range" min="0" max="100" value="100"/>
      <button class="fs">${icon("fullscreen", 18)}</button>
    </div>`;

  const video = el.querySelector<HTMLVideoElement>(".player-video")!;
  const seek = el.querySelector<HTMLInputElement>(".seek")!;
  const time = el.querySelector<HTMLElement>(".time")!;
  const fmt = (s: number) => `${Math.floor(s / 60)}:${String(Math.floor(s % 60)).padStart(2, "0")}`;
  let duration = 0;      // 真实总时长（后端 probe）
  let seekBase = 0;      // 当前流的起始偏移（seek 重起后 video.currentTime 从 0 计，需加此偏移）
  let seeking = false;
  let closed = false;

  // 起转码会话并把 <video> 指向 stream:// 流
  duration = await api.playerOpen(it.path);
  video.src = "stream://localhost/current";
  video.load();
  video.play().catch(() => {});
  // 续播
  api.getVideoPos(it.id).then((resume) => {
    if (resume > 5 && !closed) doSeek(resume);
  }).catch(() => {});

  function absPos() { return seekBase + video.currentTime; }

  async function doSeek(target: number) {
    seekBase = target;
    await api.playerSeek(it.path, target);
    video.src = "stream://localhost/current?t=" + Date.now(); // 变 url 强制重载新流
    video.load();
    video.play().catch(() => {});
  }

  el.querySelector<HTMLButtonElement>(".pp")!.onclick = () => {
    if (video.paused) { video.play(); el.querySelector(".pp")!.innerHTML = icon("pause", 18); }
    else { video.pause(); el.querySelector(".pp")!.innerHTML = icon("play", 18); }
  };
  el.querySelector<HTMLButtonElement>(".rw")!.onclick = () => doSeek(Math.max(0, absPos() - 10));
  el.querySelector<HTMLButtonElement>(".ff")!.onclick = () => doSeek(Math.min(duration, absPos() + 10));
  el.querySelector<HTMLInputElement>(".vol")!.oninput = (e) =>
    video.volume = Number((e.target as HTMLInputElement).value) / 100;
  seek.oninput = () => { seeking = true; };
  seek.onchange = () => {
    const target = (Number(seek.value) / 1000) * duration;
    doSeek(target);
    setTimeout(() => { seeking = false; }, 300);
  };
  el.querySelector<HTMLButtonElement>(".fs")!.onclick = () => {
    if (!document.fullscreenElement) video.requestFullscreen?.();
    else document.exitFullscreen?.();
  };

  video.ontimeupdate = () => {
    if (seeking || closed) return;
    const pos = absPos();
    if (duration > 0) {
      seek.value = String((pos / duration) * 1000);
      time.textContent = `${fmt(pos)} / ${fmt(duration)}`;
    }
    api.setVideoPos(it.id, pos).catch(() => {});
  };

  const cleanup = () => {
    if (closed) return;
    closed = true;
    if (absPos() > 0) api.setVideoPos(it.id, absPos()).catch(() => {});
    video.pause();
    video.src = "";
    api.playerStop().catch(() => {});
  };
  el.querySelector<HTMLButtonElement>(".back")!.onclick = () => { cleanup(); onExit(); };
  el.addEventListener("player-detach", cleanup);
  return el;
}
```

- [ ] **Step 2: CSS 调整**

`src/styles/theme.css`：把 `.player-stage`（16:9 那块）改为容纳 video：
```css
.player-video{width:100%;max-height:100%;aspect-ratio:16/9;border-radius:10px;background:#000;object-fit:contain}
```
（`.player-stage-wrap` 保留居中；`.player-stage` 规则可删或保留无害。`object-fit:contain` 保比例、黑边填充；video 自带无交通灯问题。）

- [ ] **Step 3: 编译**

Run: `npm run build 2>&1 | tail -6` — tsc+vite 通过。
Run: `cd src-tauri && cargo build 2>&1 | tail -3` — 通过。

- [ ] **Step 4: 提交**

```bash
git add -A && git commit -m "feat: DOM <video> player with transcoded stream"
```

## Task 6: 真机验证 + 流式优化（按需）

**Files:** 可能 Modify `src-tauri/src/lib.rs`（改真流式）、`src-tauri/src/player/transcode.rs`

- [ ] **Step 1: 重启应用真机测**

后端依赖变了需重启：
```bash
pkill -f "tauri dev"; pkill -f "target/debug/collector"; sleep 1
cd /Users/zhoumo/Documents/Claude/collector && nohup npm run tauri dev > /tmp/collector-dev.log 2>&1 & disown
```

- [ ] **Step 2: 验证清单**

1. 点视频 → `<video>` 在内容区播放（真 DOM 元素，无交通灯、无独立窗、无错位、充满其容器）。
2. H.264 片源秒开（remux）；HEVC 片源起转码后能播（4K 实测 2.5x 实时，应流畅）。
3. 控制条：播放/暂停、快进快退、拖进度（seek 重起转码，1-2秒后从新位置继续、不消失）、音量、全屏（video 元素全屏正常）。
4. 退出：kill ffmpeg，立即切回、不卡死；反复进出不累积僵尸进程（`pgrep ffmpeg` 应无残留）。
5. 续播：中途退出再进从上次位置继续。

- [ ] **Step 3: 若首帧等待过久（简化版读完才响应），改真流式**

若 Step 2 发现点视频后要等很久才出画面（因 lib.rs 简化版把整个流读完才响应），把 stream 协议改为**真流式**：用 `tauri::http::Response` 的流式 body（channel/OwnedResponder 分块 respond），边读 ffmpeg stdout 边发给 video。具体：用 `responder` 支持的流式接口（Tauri 2.11 的 asynchronous protocol 支持分块响应），每读一块 `read_chunk` 就发一块。以实际 API 为准实现。验证边转边播首帧快。

- [ ] **Step 4: 提交验证结果 / 流式优化**

```bash
git add -A && git commit -m "perf: stream transcoded output progressively"  # 若做了流式优化
```
无代码改动则记录验证结论即可。

---

## 自查

**1. Spec 覆盖：**
- 移除 libmpv 及所有窗口/交通灯根源 → Task 1 ✓
- 按需转码（H.264 remux / HEVC videotoolbox）→ Task 2 build_args ✓
- fMP4 流 + stream:// 协议 → Task 3 ✓
- seek 重起转码 → Task 3 player_seek + Task 5 doSeek ✓
- DOM `<video>` 内嵌 + 控制条（播放/暂停/快进快退/进度/音量/全屏）→ Task 5 ✓
- 退出 kill ffmpeg 不卡死 → Task 2 stop/Drop + Task 3 player_stop + Task 5 cleanup ✓
- 续播 → Task 5 getVideoPos/setVideoPos ✓
- 真机验证 + 4K 流畅 → Task 6 ✓

**2. 占位符扫描：** 无 TBD。Task 3/6 标注的"简化版先跑通、真流式按需优化""以实际 API 为准"是真实工程策略（先端到端通、再优化流式），有明确判断标准，非占位。

**3. 类型一致性：** 后端 command（player_open→f64 时长 / player_seek(path,secs) / player_stop）与前端 ipc（playerOpen→number / playerSeek(path,secs) / playerStop）一致；player_seek 含 secs 用 rename_all camelCase，前端传 {path, secs}（均无需转换的单词）。TranscodeSession 的 start/read_chunk/stop 在 Task 2 定义、Task 3 使用，签名一致。

自查通过。
