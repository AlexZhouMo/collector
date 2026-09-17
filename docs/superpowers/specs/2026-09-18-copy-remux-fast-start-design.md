# 视频快速起播：纯 copy remux（去除音频转码）

日期：2026-09-18

## Context（为什么做这件事）

视频库全是 **H.264 视频 + AC-3 音频 + mkv 容器，2-5GB**。此前播放需转码到 mp4 才能用 `<video>` 播，2-5GB 文件要等 ~84 秒，体验差。

之前探索过 libmpv 嵌入（真秒开，但视频在 WKWebView 之外的原生 NSView，与 HTML 控件跨图层靠坐标同步，快速 resize 会飘移，用户不接受），已放弃。

**实测定位根因**（安娜贝尔 3.6G mkv）：
- 当前 `-c:v copy -c:a aac -movflags +faststart`：**~84 秒**
- 视频本就 copy、faststart 仅几秒——**慢的唯一主凶是音频转 AAC（解码 AC-3 + 重编码整条音轨，~70 秒）**
- 改为 `-c:a copy`（音频也不转）：全片 remux **~4 秒**，约 20 倍提速

**关键前提已真机验证**：macOS WKWebView/AVFoundation **原生支持 AC-3 解码**——音频直接 copy 进 mp4，`<video>` 能正常出声（真机确认：起播快 + 有声音）。此前"WebView 不支持 AC-3 所以必须转 AAC"的假设在 macOS 上不成立。

## 方案

**`<video>` + 纯 copy remux（视频音频都不重编码）**

- 播放仍用 `<video>` 标签：视频帧合成进网页图层，**完美嵌入右侧内容区、跟随 CSS 布局零飘移、原生支持全屏**（浏览器系播放器的标准做法，从根本上没有跨图层同步问题）。
- ffmpeg 参数：`-c:v copy -c:a copy -movflags +faststart -f mp4`——纯容器转封装（mkv→mp4），moov 前置供 `<video>` 拿头即播。
- 靠 macOS 原生 AC-3 支持，免去 AAC 重编码，2-5GB 文件从 ~84s 降到 ~4s。

**为什么不用 libmpv/原生 NSView**：那会把视频放到 WebView 之外的独立图层，与 HTML 控件跨图层靠坐标同步——飘移根源，用户明确不接受。`<video>` 让视频回到网页合成器，消除飘移。

**音频健壮性**：库全为 AC-3，直接全 copy（最快、改动最小）。若未来出现 DTS/TrueHD 等 WKWebView 不支持的音频，copy 后会无声——届时再加"按音频编码分支（不支持的转 AAC）"，当前不做（YAGNI）。

## 改动

- `src-tauri/src/player/transcode.rs` 的 `fmp4_args`：`-c:a aac -b:a 192k` → `-c:a copy`（唯一实质改动）。
- 其余转码/缓存/HTTP server/前端 `<video>` 播放链路不变（复用现有架构）。

## 验证方式

- 真机点未缓存视频：起播快（~4s，非 84s）、**有声音**（AC-3 原生播放）、画面正常、可 seek、可全屏、嵌入右侧内容区无飘移。（已真机确认：速度可接受 + 有声音。）
- `cargo build` clean、`cargo test` 全过、`npx tsc --noEmit` clean。

## 回滚

本方案在分支 `feat/libmpv-instant-playback` 上开发（分支名承接上一轮探索，实际方案为纯 copy remux）。改动极小且已验证，验证通过后合并回 master；master 锚点 55682b7 可回滚。
