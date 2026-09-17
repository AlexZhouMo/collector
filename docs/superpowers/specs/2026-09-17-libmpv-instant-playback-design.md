# 视频秒开：libmpv 嵌入原生播放设计

日期：2026-09-17

## Context（为什么做这件事）

用户的视频库全是 **mkv + H.264 视频 + AC-3 音频，约 3G**。此前的播放方案（ffmpeg 转封装成 mp4 → 本地 HTTP server → WKWebView `<video>`）存在无法解决的慢：

- WKWebView/AVFoundation **不支持渐进解析 fragmented MP4**（增长中的 .part 被判 `isPlayable=false`、tracks=0），边转边播在此平台行不通（已实测确认）。
- 回退到"转码完成再播"后能播，但 3G 文件即使 arm64 ffmpeg `-c:v copy`（视频不重编码）+ `-c:a aac` + `+faststart`，仍需等整片处理到片尾（faststart 要二次遍历、音频转 AAC 走完整轨），十几到几十秒，达不到秒开。

**关键洞察**：视频流已是 H.264（MP4 兼容），无需重编码；瓶颈只在容器（mkv）和音频（AC-3）。真正的秒开不是"转码更快"，而是**用原生播放器直接播原始 mkv，零转码零 remux**——这正是 mpv/VLC 这类播放器的强项。

**目标**：点视频 1 秒内起播，支持 seek、AC-3 音频、多音轨、内挂/外挂字幕。

## 已确认的关键决策

- 播放内核：**方案 C（原生播放器）**，放弃 WKWebView `<video>` + 转码链路。
- 实现形态：**C2a —— libmpv 嵌入 Tauri 窗口的子 NSView（`--wid`）**，视频显示在应用内。
- mpv 分发：**内置 libmpv 及依赖**（像现在内置 ffmpeg 那样）。
- 字幕：**弃用 libass-wasm，改用 mpv 原生字幕**（内挂自动识别、外挂 sub-add）。
- 降级路径：C2 可行性验证（阶段 0）不通过则降级 **C1（mpv 独立窗口 + IPC 控制）**，仍能真秒开。

## 架构与技术形态（C2a）

- Rust 后端通过 FFI 调 libmpv（`libmpv.2.dylib`，libmpv API 2.5.0，本机 homebrew 已有；分发时内置）。Rust 绑定用 `libmpv2`（或 `libmpv-sys`）。
- 从 Tauri 窗口拿 `NSWindow` → 创建子 `NSView` 作为 mpv 视频层，z 序在 WKWebView **下方**；用 `objc2`/`cocoa` crate 操作原生视图。
- WKWebView 背景设**透明**，视频透过显示；播放控制条（现有 `player-bar` HTML）浮在视频上层。
- libmpv `--wid=<NSView 指针>` 把视频渲染进该视图。
- 播放控制经前端 → Tauri command → libmpv（`mpv_command`/`mpv_set_property`）。

### 数据流
1. 前端点视频 → `player_open`（新）→ Rust libmpv `loadfile <mkv 绝对路径>`（不经 HTTP server、不转码、不 remux）。
2. mpv 秒开渲染到嵌入 NSView。
3. 控制条指令 → Rust → libmpv。
4. mpv 事件（duration/time-pos/end-file）→ Rust → `emit` → 前端更新进度条/时间。

## 字幕、控制、生命周期

### 字幕（mpv 原生）
- mkv 内挂字幕：mpv 自动识别，`sid` 属性切换。
- 外挂字幕（现有 `subtitles/<分类>/<路径>/<标题>.ass` 明文文件）：`player_open` 时 `sub-add <ass 路径>`。
- CC 开关：mpv `sub-visibility` 属性。
- 弃用 `SubtitleRenderer.ts` + libass-wasm + `public/libass/`（2.2MB wasm）+ `src/assets/fonts` CJK 字体。

### 播放控制（复用 player-bar，改发 mpv 指令）
- 播放/暂停 `set_property pause`；seek `seek <秒> absolute`；音量 `set_property volume`；快退/快进 `seek ±10`。
- 进度/时长：监听 mpv `time-pos`/`duration` 事件 → emit 前端。
- 键盘快捷键（空格/方向键/Esc）保留，转 mpv 指令。

### 全屏
- 现有 Tauri 原生窗口全屏（`setFullscreen`）保留；嵌入 NSView 随窗口 resize 同步尺寸。

### 生命周期
- 打开：创建 mpv 实例 + 嵌入 NSView + loadfile。
- 退出/返回：`mpv_destroy`、移除 NSView、恢复 WKWebView 背景。
- 切换视频：复用实例 loadfile 新文件（或销毁重建）。

## 现有代码处置

- **删除/弃用**：`player/transcode.rs`、`player/httpserver.rs`、`player/ffmpeg_paths.rs`、`components/SubtitleRenderer.ts`、`public/libass/`、`src/assets/fonts` CJK 字体、`scripts/fetch-ffmpeg.mjs`（改 fetch-mpv）。
- **新增**：`player/mpv.rs`（libmpv FFI 封装：实例、loadfile、属性、事件循环）、`player/embed_macos.rs`（NSWindow/NSView 嵌入与 resize，objc2/cocoa）。
- **重写**：`player/mod.rs`（player_open/stop 改 mpv 控制）、`views/PlayerView.ts`（去 `<video>`，控制条发 mpv 指令，进度事件更新 UI）。
- **依赖新增**：`libmpv2`（或 libmpv-sys）、`objc2`/`cocoa`。

## 分阶段实现与验证

### 阶段 0：可行性验证原型（go/no-go 闸门，先做）
验证 C2 最不确定的底层假设：
1. **透明合成**：Tauri 窗口拿 NSWindow → 建子 NSView → libmpv `--wid` 播一个 mkv → WKWebView 背景透明 → 确认视频透出显示、控制条能浮在上层。
2. **libmpv FFI**：Rust 经 libmpv2 成功 loadfile 播放、set_property 控制、收到 time-pos 事件。
- **若失败** → 降级 C1（mpv 独立窗口 + IPC），仍真秒开。同阶段可并行验证 C1 作保底。

### 阶段 1：libmpv 播放核心
- `player/mpv.rs`：FFI 封装 mpv 实例、loadfile、属性读写、事件循环。
- `player/embed_macos.rs`：NSView 嵌入、resize 跟随。
- 单元测试属性/命令映射。

### 阶段 2：接入 player_open/stop + 前端控制
- `mod.rs` 重写：player_open 创建 mpv + loadfile；player_stop 销毁。
- `PlayerView.ts`：去 `<video>`，控制条发 Tauri command，进度事件更新 UI。

### 阶段 3：字幕 + 全屏 + 键盘
- 外挂 sub-add、内挂轨道切换、CC 开关、全屏 resize 同步、键盘快捷键。

### 阶段 4：清理旧链路 + 内置 mpv 分发
- 删 transcode/httpserver/ffmpeg_paths/SubtitleRenderer/libass。
- fetch-mpv 脚本 + 打包内置 libmpv 及依赖库（动态库依赖多，需打包整套或用静态 libmpv）。

## 验证方式
- 阶段 0 必须真机验证透明合成（go/no-go）。
- 每阶段实际运行。
- 最终验收：点视频 → 1 秒内看到画面、能 seek、有声音、AC-3 正常、内挂/外挂字幕正常、全屏正常。

## 风险最高、需实测的点
1. **Tauri WKWebView 透明背景 + 下层 NSView 视频合成**（最不确定，阶段 0 验证）——能否透出、z 序、控制条浮层。
2. libmpv `--wid` 嵌入 Tauri 管理的 NSView 的稳定性。
3. 窗口 resize/全屏时视频层同步。
4. 内置 libmpv 分发：动态库依赖多，打包比静态 ffmpeg 复杂（打包整套依赖或静态 libmpv）。
5. 平台范围：本设计聚焦 macOS（NSView）。Windows 打包（现有 CI）需另做 HWND 嵌入——本次不含，Windows 保留旧转码链路或后续单独处理。
