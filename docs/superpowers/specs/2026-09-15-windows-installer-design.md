# Windows 安装包（GitHub Actions 云构建）— 设计

## 背景与目标

应用 Collector（Tauri 2 + Vite/TS 前端 + Rust 后端）当前仅在 macOS 开发。目标：产出可在 **Windows** 安装并运行的安装包，要求：
- 自动检测并安装前置依赖；
- 把数据库、封面、字幕等应用目录资源一并打包，用户开箱即有内容。

### 探查澄清的关键事实（与旧 README 不符，以此为准）
- **真正的运行时前置依赖 = ffmpeg + ffprobe**（`src-tauri/src/player/transcode.rs:15,81` 用 `Command::new` 调用，做 MKV→MP4 无损 remux）。README 中大段 libmpv 内容**已过时**（早期方案，现改为 `<video>` + 本地 HTTP server + ffmpeg 子进程）。Cargo.toml 无 mpv crate、代码无 libmpv 引用、`.cargo/config.toml` 为空。
- **应用资源 ~231M**：`collector.sqlite`(1.4M) + `covers/`(89M) + `subtitles/`(141M)。`video_cache/`(6.5G) 是缓存，**不打包**。
- **不能交叉编译**：当前 macOS ARM，无 Windows target/工具链；Tauri 从 macOS 交叉编译到 Windows 实践不可行 → 用 **GitHub Actions windows runner 云构建**。
- 远程仓库：`https://github.com/AlexZhouMo/collector.git`（已存在、空、**PUBLIC**）。`gh` 已登录 AlexZhouMo。

## 方案（5 部分）

### 1. 推送 GitHub + CI 工作流
- 本地 master（351 commits）推送到 `AlexZhouMo/collector`。
- 新增 `.github/workflows/build-windows.yml`：
  - 触发：push 到 master、手动 `workflow_dispatch`、打 tag `v*`。
  - runner：`windows-latest`。步骤：checkout → 装 Node(setup-node) + Rust(dtolnay/rust-toolchain stable, target x86_64-pc-windows-msvc) → `npm ci` → `npm run tauri build`。
  - 产物：NSIS `.exe` 安装包 upload 为 artifact；打 tag 时用 `softprops/action-gh-release` 发 Release。

### 2. Windows bundle 配置（src-tauri/tauri.conf.json）
- `bundle.targets`: 明确设为 `["nsis"]`（Windows 主分发格式；避免 wix 的额外依赖）。
- 确认 `bundle.icon` 含 `icons/icon.ico`（已有）。
- `bundle.resources`: 纳入初始数据 seed 目录（见第 4 部分）。
- 新增 `bundle.windows.nsis`: 配置 installerHooks 指向自定义 NSIS 脚本（见第 3 部分）。

### 3. ffmpeg 依赖 — 安装时检测下载
- **NSIS installerHook**（`src-tauri/installer-hooks.nsi`，由 tauri.conf 的 `bundle.windows.nsis.installerHooks` 引用）：`!macro NSIS_HOOK_POSTINSTALL` 里检测 `ffmpeg -version`（`nsExec::ExecToStack`）；返回非 0（未装）则从公开源下载 ffmpeg-release-essentials（BtbN/gyan.dev 的 zip），解压 ffmpeg.exe/ffprobe.exe 到安装目录 `bin/`。
- **代码改造**（`src-tauri/src/player/transcode.rs`）：新增 `fn ffmpeg_bin(name: &str) -> String`——优先返回应用安装目录旁 `bin/<name>.exe`（用 `std::env::current_exe()` 定位），存在则用之，否则回退裸命令名（PATH）。把两处 `Command::new("ffprobe")`/`Command::new("ffmpeg")` 改为 `Command::new(ffmpeg_bin("ffprobe"))` 等。macOS/Linux 分支保持裸命令名（PATH）不变，仅 Windows 走 bin/ 优先——用 `#[cfg(windows)]` 区分或统一逻辑（bin 不存在即回退，天然兼容）。

### 4. 应用资源打包 + 首启释放
- **打包前准备**（一次性脚本 `scripts/prepare-seed.sh`，本地 macOS 跑）：把当前 `~/Library/Application Support/com.zhoumo.collector/` 的 `collector.sqlite` + `covers/` + `subtitles/` 复制到 `src-tauri/resources/seed/`（**排除 video_cache**）。这些文件随仓库提交或由 CI 前置步骤生成——因体积 231M，**提交进仓库**（PUBLIC，用户已确认接受公开）。
- **tauri.conf**：`bundle.resources`: `{"resources/seed": "seed"}`（打进安装包，运行时经 `app.path().resource_dir()` 可达）。
- **首启释放**（`src-tauri/src/lib.rs` setup 钩子或 Db 初始化处）：应用启动时检查用户数据目录 `%APPDATA%/com.zhoumo.collector/collector.sqlite` 是否存在；不存在则把 bundled `seed/` 整体复制到该目录（数据库+covers+subtitles）。已存在则跳过（不覆盖用户数据）。跨平台安全：仅当目标缺失才释放。

### 5. 验证
- CI：workflow 跑通，产出 `.exe` artifact 可下载。
- 逻辑审查：ffmpeg_bin 回退、首启释放的幂等（已存在不覆盖）。
- **能力边界（诚实标注）**：开发环境为 macOS，无法端到端实测 Windows 安装/运行。首装依赖下载、seed 释放、ffmpeg 调用的真机行为需**用户在 Windows 上实测确认**。

## 关键文件
- 新增：`.github/workflows/build-windows.yml`、`src-tauri/installer-hooks.nsi`、`scripts/prepare-seed.sh`、`src-tauri/resources/seed/*`（数据快照）。
- 改：`src-tauri/tauri.conf.json`（targets/resources/nsis）、`src-tauri/src/player/transcode.rs`（ffmpeg_bin）、`src-tauri/src/lib.rs`（首启 seed 释放）。

## 不做（YAGNI）
- 不做 macOS 交叉编译尝试（已确认不可行，走 CI）。
- 不内置 ffmpeg 随包（用户选了"安装时检测下载"）。
- 不做自动更新、代码签名/公证（Windows 无签名会有 SmartScreen 提示，属可接受范围；如需签名另议）。
- 不动 libmpv 相关旧 README 描述之外的东西（可顺带订正 README 的过时 libmpv 段，但非本任务核心）。

## 风险
- **无法本机实测 Windows**：CI 构建通过 ≠ 运行无误，最终靠用户实测。
- **seed 数据公开**：231M 含当前机器媒体库记录、封面、字幕，随 PUBLIC 仓库公开（用户已确认）。
- **首装联网**：ffmpeg 未装时需下载，无网则视频 remux 失败（但封面/字幕/浏览不受影响）。
- **NSIS 下载源稳定性**：ffmpeg 第三方下载源可能变动，需用稳定 release URL 并容错。
