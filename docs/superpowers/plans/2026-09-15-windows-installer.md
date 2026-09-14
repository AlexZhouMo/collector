# Windows 安装包 实现计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 通过 GitHub Actions windows runner 云构建，产出 Windows NSIS 安装包：安装时检测/下载 ffmpeg 前置依赖，打包当前数据快照(数据库/封面/字幕)并首启释放。

**Architecture:** macOS 无法交叉编译 → CI 云构建。代码改造 3 处(ffmpeg 路径解析、首启 seed 释放、tauri.conf)，配置 3 新文件(CI workflow、NSIS hook、prepare-seed 脚本)，seed 数据 231M 提交进 PUBLIC 仓库。

**Tech Stack:** Tauri 2、Rust、GitHub Actions(windows-latest)、NSIS、gh CLI。

---

## 关键事实（探查已确认）
- ffmpeg 调用：`src-tauri/src/player/transcode.rs:15`(`Command::new("ffprobe")`)、`:81`(`Command::new("ffmpeg")`)。
- setup 闭包：`src-tauri/src/lib.rs` `.setup(|app|{...})`，首句 `let dir = app.path().app_data_dir()...; create_dir_all(&dir)`，随后 `Db::open(&dir.join("collector.sqlite"))`。**seed 释放插在 create_dir_all 之后、Db::open 之前**。
- 远程：`https://github.com/AlexZhouMo/collector.git`（空 PUBLIC）。本地 master 351 commits。gh 已登录 AlexZhouMo。
- icon.ico 已存在于 `src-tauri/icons/`。

---

## Task 1: ffmpeg 路径解析改造（transcode.rs）

**Files:** Modify `src-tauri/src/player/transcode.rs`

- [ ] **Step 1: 加 ffmpeg_bin 辅助函数**

在 transcode.rs 顶部（use 之后）加：

```rust
/// 定位 ffmpeg/ffprobe 可执行文件：优先应用可执行文件旁的 bin/ 目录
/// （Windows NSIS 安装时把 ffmpeg 下载到此），否则回退裸命令名走 PATH。
fn ffmpeg_bin(name: &str) -> std::path::PathBuf {
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            let exe_name = if cfg!(windows) { format!("{name}.exe") } else { name.to_string() };
            let candidate = dir.join("bin").join(&exe_name);
            if candidate.exists() {
                return candidate;
            }
        }
    }
    std::path::PathBuf::from(name) // 回退 PATH
}
```

- [ ] **Step 2: 两处调用改用 ffmpeg_bin**

- `transcode.rs:15`：`Command::new("ffprobe")` → `Command::new(ffmpeg_bin("ffprobe"))`
- `transcode.rs:81`：`Command::new("ffmpeg")` → `Command::new(ffmpeg_bin("ffmpeg"))`

- [ ] **Step 3: 验证编译**

Run: `cd /Users/zhoumo/Documents/Claude/collector/src-tauri && cargo build 2>&1 | tail -3`
Expected: Finished，无错误（macOS 下 bin/ 不存在会回退裸名，行为不变）。

- [ ] **Step 4: 验证 macOS 播放行为不变（测试仍绿）**

Run: `cd /Users/zhoumo/Documents/Claude/collector/src-tauri && cargo test --lib 2>&1 | grep "test result"`
Expected: ok, 0 failed。

## Task 2: 首启 seed 释放（lib.rs setup）

**Files:** Modify `src-tauri/src/lib.rs`

- [ ] **Step 1: 加 release_seed 辅助函数**

在 lib.rs 加（migrate_* 函数附近）：

```rust
/// 首启释放：若用户数据目录无 collector.sqlite，则把 bundled seed（数据库+covers+subtitles）
/// 递归复制到 app_data。已存在则跳过（不覆盖用户数据）。seed 缺失（如无 bundle）静默跳过。
fn release_seed_if_empty(app: &tauri::App, app_data: &std::path::Path) {
    if app_data.join("collector.sqlite").exists() {
        return; // 已有数据，不覆盖
    }
    let seed = match app.path().resource_dir() {
        Ok(r) => r.join("seed"),
        Err(_) => return,
    };
    if !seed.exists() { return; }
    fn copy_dir(src: &std::path::Path, dst: &std::path::Path) -> std::io::Result<()> {
        std::fs::create_dir_all(dst)?;
        for e in std::fs::read_dir(src)? {
            let e = e?; let p = e.path(); let d = dst.join(e.file_name());
            if p.is_dir() { copy_dir(&p, &d)?; } else { std::fs::copy(&p, &d)?; }
        }
        Ok(())
    }
    if let Err(e) = copy_dir(&seed, app_data) {
        eprintln!("[seed] 释放失败: {e}");
    }
}
```

- [ ] **Step 2: 在 setup 闭包调用（Db::open 之前）**

在 `.setup(|app| {` 里，`std::fs::create_dir_all(&dir).ok();` 之后、`create_dir_all(dir.join("covers"))` 之前插入：

```rust
            release_seed_if_empty(app, &dir);
```

（放在 covers/subtitles 建目录前，避免建了空目录导致 seed 判断误差；seed 复制会自建这些子目录。）

- [ ] **Step 3: 验证编译 + 测试**

Run: `cd /Users/zhoumo/Documents/Claude/collector/src-tauri && cargo build 2>&1 | tail -2 && cargo test --lib 2>&1 | grep "test result"`
Expected: 编译通过、测试 ok（macOS 下 app_data 已有 sqlite → release_seed 直接 return，行为不变）。

## Task 3: tauri.conf.json Windows bundle 配置

**Files:** Modify `src-tauri/tauri.conf.json`

- [ ] **Step 1: 配置 bundle**

把 `bundle` 段改为（保留 active/icon，改 targets、加 resources 与 windows.nsis）：

```json
  "bundle": {
    "active": true,
    "targets": "all",
    "icon": [
      "icons/32x32.png",
      "icons/128x128.png",
      "icons/128x128@2x.png",
      "icons/icon.icns",
      "icons/icon.ico"
    ],
    "resources": {
      "resources/seed": "seed"
    },
    "windows": {
      "nsis": {
        "installerHooks": "installer-hooks.nsi"
      }
    }
  }
```

（`targets` 保持 "all"：macOS 上打 macOS 产物、CI Windows 上自动打 nsis，互不影响。resources 映射 `resources/seed`→安装后 `seed/`。）

- [ ] **Step 2: 验证 JSON 合法 + tauri 识别**

Run: `cd /Users/zhoumo/Documents/Claude/collector && python3 -c "import json;json.load(open('src-tauri/tauri.conf.json'));print('JSON OK')"`
Expected: `JSON OK`。（installer-hooks.nsi 与 resources/seed 在后续 Task 创建；macOS 本地 build 不强制需要它们存在，CI 才用。）

## Task 4: NSIS installer hook（ffmpeg 检测下载）

**Files:** Create `src-tauri/installer-hooks.nsi`

- [ ] **Step 1: 写 NSIS hook**

```nsi
; 安装后钩子：检测 ffmpeg，未装则下载 essentials 到 $INSTDIR\bin
!macro NSIS_HOOK_POSTINSTALL
  ; 检测系统 PATH 里是否已有 ffmpeg
  nsExec::ExecToStack 'cmd /c ffmpeg -version'
  Pop $0 ; 退出码
  ${If} $0 != 0
    DetailPrint "未检测到 ffmpeg，正在下载运行时依赖..."
    CreateDirectory "$INSTDIR\bin"
    ; 稳定 release 源（BtbN 静态构建 essentials）
    inetc::get /caption "下载 ffmpeg" /cancel \
      "https://github.com/BtbN/FFmpeg-Builds/releases/download/latest/ffmpeg-master-latest-win64-gpl.zip" \
      "$INSTDIR\ffmpeg.zip" /end
    Pop $1
    ${If} $1 == "OK"
      nsExec::ExecToLog 'powershell -NoProfile -Command "Expand-Archive -Force \"$INSTDIR\ffmpeg.zip\" \"$INSTDIR\ffmpeg_tmp\""'
      ; 从解压目录 bin 拷 ffmpeg.exe/ffprobe.exe 到 $INSTDIR\bin
      nsExec::ExecToLog 'powershell -NoProfile -Command "Get-ChildItem -Recurse \"$INSTDIR\ffmpeg_tmp\" -Include ffmpeg.exe,ffprobe.exe | ForEach-Object { Copy-Item $_.FullName \"$INSTDIR\bin\" -Force }"'
      Delete "$INSTDIR\ffmpeg.zip"
      RMDir /r "$INSTDIR\ffmpeg_tmp"
      DetailPrint "ffmpeg 已安装到 $INSTDIR\bin"
    ${Else}
      DetailPrint "ffmpeg 下载失败（$1），视频转码功能将不可用，可稍后手动安装 ffmpeg。"
    ${EndIf}
  ${EndIf}
!macroend
```

- [ ] **Step 2: 记录 hook 依赖**

在文件顶部注释说明：依赖 NSIS inetc 插件（tauri 打包的 NSIS 通常自带；若 CI 报缺插件，改用 `NSISdl::download` 或 powershell Invoke-WebRequest 下载）。此为纯配置文件，无本地可验证步骤——由 CI 构建时 makensis 校验语法。

## Task 5: prepare-seed 脚本 + 生成 seed 数据

**Files:** Create `scripts/prepare-seed.sh`

- [ ] **Step 1: 写脚本**

```bash
#!/usr/bin/env bash
# 把当前机器的应用数据快照复制为打包 seed（排除 video_cache 缓存）。
# 本地 macOS 运行一次，产物 src-tauri/resources/seed/ 提交进仓库供 CI 打包。
set -euo pipefail
SRC="$HOME/Library/Application Support/com.zhoumo.collector"
DST="$(cd "$(dirname "$0")/.." && pwd)/src-tauri/resources/seed"
rm -rf "$DST"
mkdir -p "$DST"
cp "$SRC/collector.sqlite" "$DST/collector.sqlite"
cp -R "$SRC/covers" "$DST/covers"
cp -R "$SRC/subtitles" "$DST/subtitles"
# 不复制 video_cache（缓存，6.5G）
echo "seed 生成于 $DST"
du -sh "$DST"
```

- [ ] **Step 2: 运行生成 seed**

Run: `chmod +x /Users/zhoumo/Documents/Claude/collector/scripts/prepare-seed.sh && /Users/zhoumo/Documents/Claude/collector/scripts/prepare-seed.sh`
Expected: 打印 seed 路径 + 体积 ~231M；`src-tauri/resources/seed/` 下有 collector.sqlite、covers/、subtitles/。

- [ ] **Step 3: 确认 seed 内容**

Run: `ls /Users/zhoumo/Documents/Claude/collector/src-tauri/resources/seed/ && du -sh /Users/zhoumo/Documents/Claude/collector/src-tauri/resources/seed/`
Expected: 三项俱全，体积合理（<300M）。

## Task 6: GitHub Actions workflow

**Files:** Create `.github/workflows/build-windows.yml`

- [ ] **Step 1: 写 workflow**

```yaml
name: Build Windows Installer
on:
  push:
    branches: [master]
    tags: ['v*']
  workflow_dispatch:
jobs:
  build:
    runs-on: windows-latest
    steps:
      - uses: actions/checkout@v4
      - uses: actions/setup-node@v4
        with:
          node-version: 20
      - uses: dtolnay/rust-toolchain@stable
        with:
          targets: x86_64-pc-windows-msvc
      - name: Install frontend deps
        run: npm ci
      - name: Build Tauri (NSIS)
        run: npm run tauri build -- --bundles nsis
      - name: Upload installer
        uses: actions/upload-artifact@v4
        with:
          name: Collector-Windows-Installer
          path: src-tauri/target/release/bundle/nsis/*.exe
      - name: Release on tag
        if: startsWith(github.ref, 'refs/tags/v')
        uses: softprops/action-gh-release@v2
        with:
          files: src-tauri/target/release/bundle/nsis/*.exe
```

- [ ] **Step 2: 校验 YAML**

Run: `cd /Users/zhoumo/Documents/Claude/collector && python3 -c "import yaml,sys;yaml.safe_load(open('.github/workflows/build-windows.yml'));print('YAML OK')" 2>/dev/null || echo "无pyyaml, 跳过(GitHub会校验)"`
Expected: `YAML OK` 或跳过提示。

## Task 7: 推送 + 触发 CI + 验证

**Files:** git 操作

- [ ] **Step 1: 提交所有改动**

```bash
cd /Users/zhoumo/Documents/Claude/collector
git add -A
git commit -m "feat(windows): GitHub Actions云构建Windows安装包+ffmpeg检测下载+seed首启释放

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>"
```
（注意 seed 231M 会进本次提交；确认 .gitignore 未忽略 resources/seed。）

- [ ] **Step 2: 加远程并推送**

```bash
cd /Users/zhoumo/Documents/Claude/collector
git remote add origin https://github.com/AlexZhouMo/collector.git 2>/dev/null || git remote set-url origin https://github.com/AlexZhouMo/collector.git
git push -u origin master
```
Expected: 推送成功（351+ commits + seed）。

- [ ] **Step 3: 确认 CI 触发**

Run: `cd /Users/zhoumo/Documents/Claude/collector && gh run list --limit 3`
Expected: 出现 Build Windows Installer 运行记录（queued/in_progress）。

- [ ] **Step 4: 等 CI 完成 + 取产物**

Run: `gh run watch $(gh run list --workflow=build-windows.yml --limit 1 --json databaseId -q '.[0].databaseId')`
Expected: 构建成功；artifact `Collector-Windows-Installer` 含 `.exe`。若失败，读日志修（常见：NSIS inetc 插件缺失 → 改下载方式；seed 太大导致 checkout 慢 → 可接受）。

## 验证总结
- CI 绿灯、artifact 产出 `.exe`。
- **能力边界**：macOS 无法实测 Windows 安装/运行；首装 ffmpeg 下载、seed 释放、播放需**用户在 Windows 实测**。计划在最终交付时明确告知用户下载 artifact 并实测。

## Self-Review
- Spec 覆盖：ffmpeg检测下载(Task4)✓ 资源打包(Task5)+首启释放(Task2)✓ Windows可装(Task3/6)✓ 云构建(Task6/7)✓。
- 无占位符：所有代码/配置/命令均完整给出。
- 一致性：ffmpeg_bin(Task1) 找 `bin/` ↔ NSIS hook(Task4) 下载到 `$INSTDIR\bin` ↔ resource_dir seed(Task2) ↔ tauri.conf resources(Task3) 路径对齐。
- 风险已在 spec 标注：无法本机实测、seed 公开、首装联网、NSIS 插件可能需调整。
