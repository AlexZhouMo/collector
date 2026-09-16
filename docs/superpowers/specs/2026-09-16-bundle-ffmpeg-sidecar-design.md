# 将 ffmpeg/ffprobe 内置到 macOS + Windows 安装包

**日期**：2026-09-16
**状态**：设计已确认，待实现

## 背景与目标

Collector（Tauri 2.x 桌面应用）的视频播放依赖 `ffmpeg`/`ffprobe` 做无损转封装（`-c copy` 将 MKV remux 成 MP4，音频转 AAC），再由前端经 `asset://` 用 `<video>` 播放。

当前 ffmpeg 是**运行时外部依赖**，三平台策略如下：

- **macOS**：让用户自行 `brew install ffmpeg`
- **Windows**：NSIS 安装时检测系统 PATH，未装则征得同意后**联网下载** BtbN 静态构建到 `$INSTDIR\bin`
- **Linux**：用户用包管理器安装

定位逻辑 `ffmpeg_bin`（`src-tauri/src/player/transcode.rs`）优先查「可执行文件旁的 `bin/` 目录」，否则回退裸命令名走 PATH。

**目标**：把 ffmpeg/ffprobe 直接**打包进 macOS 与 Windows 安装包**，让用户无需自行安装、也无需安装时联网下载。Linux 本次不处理。

## 关键决策

| 决策项 | 选择 |
| --- | --- |
| 目标平台 | **macOS + Windows**（Linux 不处理） |
| 二进制来源 | **完整静态构建**（体积不敏感）；**不在意 GPL 许可** |
| 二进制入库 | **不入 git**，构建前脚本自动下载到 `src-tauri/bin/` |
| 打包机制 | **Tauri 官方 externalBin（sidecar）机制** |
| Rust 定位方式 | 启动时解析 sidecar 绝对路径，存入 `OnceLock` 全局供 `transcode.rs` 读取 |

## 架构概览

利用 Tauri 的 **externalBin（sidecar）** 机制打包 ffmpeg/ffprobe：

1. **构建前**：脚本按平台下载完整静态二进制，重命名为 Tauri 要求的 `<name>-<target-triple>` 格式放入 `src-tauri/bin/`。
2. **打包时**：Tauri 依据 `bundle.externalBin` 声明，自动将匹配当前构建 target 的二进制纳入 `.app`/NSIS 安装包，并处理签名归属。
3. **运行时**：后端在启动时解析 sidecar 的实际文件路径（Tauri 会为二进制加 `-<target-triple>` 后缀）并缓存，`transcode.rs` 用 `std::process::Command` 调用（沿用现有调用逻辑，不引入 shell 插件）。

结果：macOS 不再需要 brew，Windows 不再需要安装时联网下载。

### 为何不引入 tauri-plugin-shell

sidecar 有两种用法：(1) 经 `tauri-plugin-shell` 的 API 调用；(2) 仅用 `externalBin` 声明打包，在 Rust 里定位后照旧用 `std::process::Command`。本场景需要在 Rust 后端 spawn、读取 stdout、传 `-y` 等参数，方案 (2) 改动更小、无需新增插件与权限，故采用 (2)。

## 组件与改动

### 1. 下载脚本 `scripts/fetch-ffmpeg.mjs`（Node，跨平台）

- 按参数（或当前平台/架构）下载对应完整静态构建：
  - **macOS**：从 evermeet.cx 分别下载 `ffmpeg`、`ffprobe`
  - **Windows**：从 BtbN `ffmpeg-master-latest-win64-gpl.zip` / `winarm64-gpl.zip` 解压出 `ffmpeg.exe`、`ffprobe.exe`
- 解压后重命名为 Tauri sidecar 要求的格式，放入 `src-tauri/bin/`：
  - `ffmpeg-aarch64-apple-darwin`、`ffprobe-aarch64-apple-darwin`（及 `x86_64-apple-darwin`）
  - `ffmpeg-x86_64-pc-windows-msvc.exe`、`ffprobe-x86_64-pc-windows-msvc.exe`（及 `aarch64-pc-windows-msvc`）
- macOS 二进制 `chmod +x`
- 幂等：已存在且能 `-version` 成功则跳过下载
- 暴露 `npm run fetch:ffmpeg`，并挂到 `beforeBuildCommand` 之前（或在文档/CI 中要求先执行）

### 2. Tauri 配置 `src-tauri/tauri.conf.json`

- 新增 `bundle.externalBin: ["bin/ffmpeg", "bin/ffprobe"]`
- **移除** `bundle.windows.nsis.installerHooks`（不再联网下载）
- 删除 `src-tauri/installer-hooks.nsi` 文件

### 3. Rust 后端 `src-tauri/src/player/transcode.rs`

- **路径解析**：在应用 `setup` 时解析 ffmpeg/ffprobe 两个 sidecar 的绝对路径（含 target-triple 后缀），存入全局 `OnceLock`。`transcode.rs` 从该全局读取，保持模块内函数可单测。
- 重写 `ffmpeg_bin`：读取 `OnceLock` 中的解析结果；未解析时回退裸命令名（兼容 `cargo test` / `tauri dev` 无 sidecar 的场景）。
- 3 处调用点沿用不变的 spawn 逻辑：`ensure_ffmpeg_available`（探活兜底）、`probe_duration`、`remux`。
- `ffmpeg_missing_hint`：简化为「内置的 ffmpeg 组件缺失或损坏，请尝试重新安装 Collector」，删除 brew/BtbN 手动安装引导。
- 更新对应单测：`ffmpeg_hint_contains_guidance` 断言改为匹配新文案。

### 4. `.gitignore`

- 忽略 `src-tauri/bin/ffmpeg*`、`src-tauri/bin/ffprobe*`。

### 5. README + CI

- README「开发依赖 / 打包」：ffmpeg 从「用户自行安装」改为「构建前运行 `npm run fetch:ffmpeg` 自动获取」，说明 macOS/Windows 已内置、用户无需再装。
- 更新面向用户的说明：删除「brew install / 安装时下载」相关描述。
- 若 `.github` 有 build workflow，在 `tauri build` 前加一步执行 `npm run fetch:ffmpeg`（按 matrix 平台/架构获取）。

## 数据流（运行时）

```
应用启动(setup) ──解析 sidecar 绝对路径──▶ OnceLock<{ffmpeg, ffprobe}>
                                              │
播放请求 ──▶ remux() ──▶ ffmpeg_bin("ffprobe")─┤─▶ 从 OnceLock 读绝对路径
                    │                          └─▶ 未初始化则回退裸命令名
                    ├─▶ ensure_ffmpeg_available()  (探活兜底)
                    ├─▶ probe_duration()           (ffprobe 取时长)
                    └─▶ Command(ffmpeg) -c:v copy -c:a aac ... ──▶ 缓存 MP4
```

## 错误处理

- 内置二进制理论上恒可用；`ensure_ffmpeg_available` 保留为兜底，失败时返回简化后的中文提示（引导重装）。
- 下载脚本失败（网络/校验）应以非零退出并打印清晰错误，避免打出缺二进制的残包。

## 测试

- 保留并更新 `transcode.rs` 现有单测（缓存路径稳定性、LRU、提示文案）。
- `ffmpeg_bin` 在无 `OnceLock` 初始化时回退裸命令名，保证 `cargo test` 正常。
- 手动验证：macOS 打 `.app`、Windows 打 NSIS，均在**无系统 ffmpeg** 的环境下能正常播放视频。

## 非目标（YAGNI）

- 不做 Linux 打包。
- 不做精简/LGPL 构建（体积不敏感，用完整 GPL 构建）。
- 不引入 tauri-plugin-shell。
- 不做 ffmpeg 版本自动升级/更新机制。
