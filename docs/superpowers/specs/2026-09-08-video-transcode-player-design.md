# 视频播放改为 ffmpeg 转码 + DOM `<video>` 内嵌 设计文档

- 日期：2026-09-08
- 状态：设计待复审

## Context

视频播放先后尝试了 libmpv 独立窗口"定位覆盖"、透明窗口、NSView 子视图嵌入、16:9 自适应共四轮，均未彻底解决：实机出现错位、白框、遮挡、鼠标划过冒出 macOS 交通灯（红黄绿）、拖进度条视频消失、画面不充满。**根因**：libmpv 在 macOS 上把渲染 wid 当作独立 NSWindow 处理（交通灯是铁证），无论怎么设子视图/坐标都绕不过——这是 libmpv + Tauri WKWebView 在 macOS 的架构性冲突。

**彻底方案**：抛弃 libmpv 播放路径，视频改为 DOM 里真正的 `<video>` 元素。由于片源含 4K HEVC（WebView 的 `<video>` 不支持 H.265），后端用 ffmpeg **按需转码**（VideoToolbox 硬件编码）成浏览器可播的 H.264/fMP4，经 Tauri 自定义协议流式喂给 `<video>`。从架构上根除所有窗口/坐标/交通灯/seek 问题——`<video>` 就是页面里的一块普通元素，天然内嵌、天然跟随布局、天然 seek。

## 已确认的设计决策

- **转码策略：按需**。视频已是 H.264 → 直接 remux 转封装（不重编码、瞬时）；HEVC 等不兼容编码 → VideoToolbox 硬编转 H.264，保持原分辨率。音频能 copy 就 copy，否则转 AAC。（保留降 1080p 的开关，默认不降；若 4K 实时转码跟不上可后续开启。）
- **seek：重起转码**。拖进度时后端用 `ffmpeg -ss <位置>` 从最近关键帧重开一个转码进程，`<video>` 重载新流。有 1-2 秒重起延迟，但不会消失。
- **输出格式**：fragmented MP4（fMP4，`-movflags frag_keyframe+empty_moov+default_base_moof`），支持流式渐进播放。
- **传输**：Tauri 自定义协议（如 `stream://`）把 ffmpeg stdout 的 fMP4 流式返回给 `<video src>`。

## 架构

移除现有 mpv 播放子系统，新增 ffmpeg 转码流子系统。

### 移除
- `src-tauri/src/player/mpv.rs`、`embed_macos.rs`（libmpv + NSView 嵌入）
- `libmpv2` 依赖、`objc2*` 依赖、macos-private-api/transparent 相关
- `player_embed`/`player_set_bounds`/`player_close_window`/`player_load`/`player_pause`/`player_seek`/`player_seek_to`/`player_volume`/`player_progress`/`player_fullscreen` 等基于 mpv 的 command
- 前端 PlayerView 里所有 mpv 定位/嵌入逻辑（stageBounds/ResizeObserver/openPlayerWindow 等）

### 新增
- **`src-tauri/src/player/transcode.rs`**：转码会话管理。
  - 探测源视频编码（复用 ffprobe 或 ffmpeg，判断视频是否 H.264、音频是否 AAC）
  - 起 ffmpeg 进程：按需选择 `-c:v copy`（已是 H.264）或 `-c:v h264_videotoolbox`（HEVC 等），`-c:a copy` 或 `aac`，输出 fMP4 到 stdout
  - `-ss <secs>` 支持从指定位置起转（seek）
  - 管理当前会话（一个视频同时只有一个转码进程）：新 seek/新视频时 kill 旧进程再起新的——**这也解决退出卡死**（退出即 kill ffmpeg 子进程，无 libmpv 卸载阻塞）
- **`src-tauri/src/player/mod.rs`**：注册自定义协议 `stream://`，把请求路由到当前转码会话的 fMP4 输出流；command：`player_open(path, subtitle)` 起会话、`player_seek(secs)` 重起转码、`player_stop()` kill 会话。
- **前端 `src/views/PlayerView.ts`**：用 `<video>` 元素 + 自定义玻璃控制条。
  - `<video>` 的 src 指向 `stream://current`（自定义协议）
  - 控制条：播放/暂停（video.play/pause）、快进快退（video.currentTime±10）、进度条（拖动→调 `player_seek` 重起转码 + video 重载）、音量（video.volume）、全屏（video 元素 requestFullscreen——现在是 DOM 元素，全屏正常）
  - 时间/进度：用 video 的 timeupdate/duration（duration 需从后端 ffprobe 拿真实总时长，因为转码流的 duration 可能不准）
  - 字幕：外挂 .ass 转成 WebVTT 喂给 `<track>`（ass→vtt 需转换；或初期先不显示字幕，后续加）
  - 续播/进度记忆：复用现有 set_video_pos/get_video_pos

## 关键文件
- `src-tauri/src/player/transcode.rs`（新增，ffmpeg 会话）
- `src-tauri/src/player/mod.rs`（改造：stream 协议 + player_open/seek/stop）
- `src-tauri/src/lib.rs`（注册协议、command；移除 mpv command）
- `src-tauri/Cargo.toml`（移除 libmpv2/objc2；ffmpeg 用系统 CLI 通过 std::process 调用，无需 crate）
- `src/views/PlayerView.ts`（改为 `<video>` + 控制条）
- `src/lib/ipc.ts`（player command 改名/调整）
- `src-tauri/tauri.conf.json`（移除 macOSPrivateApi/transparent；注册 stream 协议 CSP/权限）

## 依赖与前提
- **ffmpeg CLI**：开发机已有 `/opt/homebrew/bin/ffmpeg`+`ffprobe`。分发时需随包带 ffmpeg 二进制（替代之前的 libmpv 分发）——比 libmpv 打包简单（单个可执行）。
- 字幕 ass→vtt：初期可先跳过字幕显示（保证播放先跑通），字幕作为后续增量。

## 明确不做（YAGNI）
- 不做全片预转码（seek 重起转码已够用）
- 不做多分辨率自适应/HLS 多档（单流按需转码）
- 初期不做字幕（先跑通播放，字幕后续）
- 不再保留任何 libmpv/mpv 代码路径

## 风险
- **4K HEVC 硬编实时性**：VideoToolbox 硬编 4K→H.264 可能仍跟不上实时播放（取决于 GPU）。缓解：保留降 1080p 开关；若严重，转码时降分辨率。这是本方案最大不确定性，需真机验证转码速度。
- **自定义协议流式 + seek**：ffmpeg stdout 流式喂 `<video>` + seek 重起，需处理流中断/重连、Content-Type、Range 请求。是本方案主要工程量。

## 验证
1. 后端能起 ffmpeg 把样本 4K HEVC 转 H.264 fMP4 到 stdout，测转码速度（是否≥1x 实时）。
2. 前端 `<video>` 能通过 stream:// 播放该流。
3. 播放/暂停/快进快退/音量/全屏（DOM 元素，无窗口问题）。
4. 拖进度 → seek 重起转码 → video 从新位置继续，不消失、不卡死。
5. 退出 → kill ffmpeg，立即切回，反复进出不累积。
6. 无交通灯、无白框、无错位、video 充满其容器（DOM 元素天然如此）。
