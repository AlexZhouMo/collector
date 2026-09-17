# 视频秒开：libmpv 嵌入原生播放 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 用 libmpv 嵌入 Tauri 窗口直接播放原始 mkv（h264+ac3），实现零转码秒开；替换现有 ffmpeg 转码 + HTTP server + `<video>` + libass 播放链路。

**Architecture:** macOS 上从 Tauri 窗口取 NSWindow，创建子 NSView 作 mpv 视频层（z 序在透明 WKWebView 下），libmpv `--wid` 渲染其中；播放控制经前端→Tauri command→libmpv。阶段 0 是 go/no-go 闸门：验证不通过则降级 C1（mpv 独立窗口 + IPC）。

**Tech Stack:** Rust FFI（libmpv2/libmpv-sys）、objc2/cocoa（NSView）、Tauri 2、libmpv 2.5.0、TypeScript。

**重要说明（关于本计划的性质）：** 阶段 0 是**探索性可行性验证**，其目标是确认底层假设、摸清 libmpv2/objc2 的确切 API，而非执行既定代码——故阶段 0 的步骤以"验证目标 + 判定标准"表述，具体 FFI 代码在实现时对照 crate 文档确定。阶段 1+ 在阶段 0 摸清 API 后才有确定的代码形态；本计划先详列阶段 0 与阶段 1 骨架，阶段 0 通过后应回来补细化阶段 2-4（或据阶段 0 结论调整）。

**验证命令基线：**
- 后端：`cd /Users/zhoumo/Documents/Claude/collector/src-tauri && cargo build && cargo test`
- 前端：`cd /Users/zhoumo/Documents/Claude/collector && npx tsc --noEmit`
- 运行：`npm run tauri:dev`（真机看视频窗口）

---

## 阶段 0：可行性验证原型（GO / NO-GO 闸门）

**这是决定性阶段。** 目标：用最小代价验证 C2 最不确定的两个底层假设。任一不通过 → 记录结论 → 降级 C1（见末尾降级说明）。不要在阶段 0 未通过时进入阶段 1。

### Task 0.1: 确认 libmpv Rust 绑定可用并能无窗口播放

**Files:**
- Modify: `src-tauri/Cargo.toml`（加 libmpv 绑定依赖）
- Create: `src-tauri/src/player/mpv_probe.rs`（临时验证模块，阶段 0 用，通过后并入 mpv.rs 或删除）

- [ ] **Step 1: 选定并加入 libmpv 绑定依赖**

调研 `libmpv2`（维护较活跃）与 `libmpv-sys`（裸 FFI）两个 crate 的当前版本与 API。优先 `libmpv2`。在 `src-tauri/Cargo.toml` 的 `[dependencies]` 加入（版本以 crates.io 实际最新为准，实现时确认）：
```toml
libmpv2 = "..."   # 确认版本；若不可用改用 libmpv-sys 裸 FFI
```
确保链接到本机 libmpv：`/opt/homebrew/lib/libmpv.dylib`（build 时可能需 `build.rs` 加 `println!("cargo:rustc-link-search=/opt/homebrew/lib")`）。

- [ ] **Step 2: 写最小验证——无窗口 loadfile 播放**

在 `mpv_probe.rs` 写一个 `#[test]` 或临时命令：创建 mpv 实例、`loadfile` 一个真实 mkv（`/Users/zhoumo/Downloads/电影/恐怖/安娜贝尔/[2019].安娜贝尔3：回家.mkv`）、读 `duration` 属性、收到播放事件。目的只验证 FFI 链路通、libmpv 能解码该 mkv。

判定标准：能创建实例、loadfile 不报错、能读到 duration（约 6365 秒）。

- [ ] **Step 3: 编译并运行验证**

Run: `cd /Users/zhoumo/Documents/Claude/collector/src-tauri && cargo build 2>&1 | tail -20`
Expected: 编译通过（若链接 libmpv 失败，调 build.rs 的 link-search/link-lib）。
运行验证测试，确认能读到 duration。

判定：**通过** → Task 0.2；**FFI 完全不通/无可用绑定** → 记录，考虑 C1（C1 用子进程 mpv，无需 FFI）。

### Task 0.2: 验证 Tauri 窗口 NSView 嵌入 + WKWebView 透明合成

**Files:**
- Modify: `src-tauri/Cargo.toml`（加 objc2/cocoa）
- Create: `src-tauri/src/player/embed_probe.rs`（临时验证）
- Modify: `src-tauri/src/lib.rs`（临时挂一个验证命令）

- [ ] **Step 1: 加原生视图操作依赖**

`Cargo.toml` 加 `objc2` + `objc2-app-kit`（或 `cocoa` + `objc`，选与 Tauri 2 兼容的当前版本）。

- [ ] **Step 2: 从 Tauri 窗口取 NSWindow 并建子 NSView**

写一个临时 Tauri command：入参 Tauri `Window`，通过 `window.ns_window()`（Tauri 2 提供，返回 `*mut c_void` 指向 NSWindow）取原生窗口，创建一个子 NSView 覆盖内容区，插入到 WKWebView **下方**（`contentView` 的 subview 顺序）。

- [ ] **Step 3: libmpv --wid 渲染到该 NSView + WebView 透明**

把 Step 2 的 NSView 指针作为 libmpv `wid` 属性（或创建 mpv 时 `--wid=<ptr>`），loadfile 播放。同时把 WKWebView 背景设透明（Tauri 2：`window` 创建时 `transparent(true)` 或运行时设 NSWindow/WKWebView opaque=false、背景 clear）。

- [ ] **Step 4: 真机验证透明合成（GO/NO-GO 核心）**

Run: `npm run tauri:dev`，触发验证命令，**人眼确认**：
1. mkv 视频画面显示在应用窗口内（透过透明 WebView 露出下层 NSView）。
2. 前端 HTML 元素（如一个测试按钮/控制条）能浮在视频上层可见可点。
3. 视频正常解码播放（h264+ac3）。

判定：
- **三点都满足 → C2 可行，GO，进入阶段 1。**
- **视频不显示/被 WebView 遮挡/合成异常 → C2 在本环境不可行，NO-GO → 降级 C1**（见末尾）。

- [ ] **Step 5: 记录阶段 0 结论**

把验证结果（GO 或 NO-GO + 原因）写入 `docs/superpowers/specs/2026-09-17-libmpv-instant-playback-design.md` 末尾追加"## 阶段 0 验证结论"小节，并 git 提交：
```bash
cd /Users/zhoumo/Documents/Claude/collector
git add docs/superpowers/specs/2026-09-17-libmpv-instant-playback-design.md src-tauri/Cargo.toml
git commit -m "spike(player): 阶段0 libmpv 嵌入可行性验证结论

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>"
```

**闸门：仅当 Task 0.2 判定 GO 才继续阶段 1。NO-GO 则停下与用户确认切换 C1。**

---

## 阶段 1：libmpv 播放核心（阶段 0 GO 后）

> 说明：以下为骨架任务。阶段 0 摸清 libmpv2/objc2 确切 API 后，实现时按真实 API 补齐每步代码。

### Task 1.1: libmpv 封装 mpv.rs

**Files:**
- Create: `src-tauri/src/player/mpv.rs`
- Modify: `src-tauri/src/player/mod.rs`（`pub mod mpv;`）

- [ ] **Step 1: 定义 mpv 播放器封装结构**

`MpvPlayer` 持有 mpv 实例 handle，提供方法：`new(wid: NSView 指针) -> Result<Self>`、`load_file(path)`、`set_pause(bool)`、`seek_absolute(secs)`、`set_volume(0-100)`、`set_sub_visibility(bool)`、`add_subtitle(path)`、`get_duration() -> f64`、`get_time_pos() -> f64`。每个方法映射到 libmpv command/property。

- [ ] **Step 2: 事件循环线程**

起后台线程跑 mpv 事件循环（`wait_event`），把 `duration`/`time-pos`/`end-file` 事件通过 channel 或直接 `app.emit` 转给前端（事件名如 `mpv-progress`、`mpv-ended`）。

- [ ] **Step 3: 单元测试属性/命令映射**

对可纯逻辑测试的部分（如秒数→seek 命令字符串、音量范围钳制）写单测。FFI 部分靠阶段 0/集成验证。

- [ ] **Step 4: 编译**

Run: `cargo build 2>&1 | grep -iE "error|warning" || echo clean` → clean。

- [ ] **Step 5: 提交**
```bash
git add src-tauri/src/player/mpv.rs src-tauri/src/player/mod.rs
git commit -m "feat(player): libmpv 播放器封装 mpv.rs

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>"
```

### Task 1.2: NSView 嵌入 embed_macos.rs

**Files:**
- Create: `src-tauri/src/player/embed_macos.rs`

- [ ] **Step 1: 把阶段 0 验证的 NSView 创建/嵌入/透明代码固化为函数**

`create_video_view(window) -> NSView 指针`、`remove_video_view(...)`、`resize_video_view(...)`（窗口 resize/全屏时调用）。用阶段 0 已验证可行的方式实现。

- [ ] **Step 2: 编译 + 提交**
```bash
git add src-tauri/src/player/embed_macos.rs
git commit -m "feat(player): macOS NSView 视频层嵌入与 resize

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>"
```

---

## 阶段 2：接入 player_open/stop + 前端控制

### Task 2.1: 重写 player/mod.rs 的 command

**Files:**
- Modify: `src-tauri/src/player/mod.rs`
- Modify: `src-tauri/src/lib.rs`（PlayerState 改持 MpvPlayer；invoke_handler 增删命令）

- [ ] **Step 1: PlayerState 改持 MpvPlayer**

`PlayerState(Mutex<Option<MpvPlayer>>)`。`player_open`：解析 mkv 绝对路径（复用现有 `library::paths::video_abs_path`）→ 创建 video NSView（embed_macos）→ 创建 MpvPlayer(wid) → load_file → 挂外挂字幕（若存在）→ 返回 duration。不再有转码/HTTP/src。

- [ ] **Step 2: 新增控制 command**

`player_pause(bool)`、`player_seek(secs)`、`player_volume(v)`、`player_set_subtitle(bool)`、`player_stop`（销毁 mpv + 移除 NSView）。在 lib.rs `invoke_handler` 注册。

- [ ] **Step 3: PlayerInfo 精简**

去掉 src/progressive/epoch（不再用 HTTP/转码）；保留 duration、subtitle（是否有字幕轨）。

- [ ] **Step 4: 编译 + 测试**

Run: `cargo build ... || echo clean`；`cargo test`（现有非 player 测试仍应通过；被删模块的测试一并移除）。

- [ ] **Step 5: 提交**
```bash
git add src-tauri/src/player/mod.rs src-tauri/src/lib.rs
git commit -m "feat(player): player_open/stop 改为 libmpv 控制，去转码链路

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>"
```

### Task 2.2: 重写 PlayerView.ts

**Files:**
- Modify: `src/views/PlayerView.ts`
- Modify: `src/lib/ipc.ts`（playerOpen 返回类型改、增控制 API）

- [ ] **Step 1: 去掉 `<video>` 与 HTTP src 逻辑**

PlayerView 不再有 `<video>` 元素、进度监听、转码状态机。改为：调 `player_open` → mpv 在嵌入层播放 → 控制条按钮发 `player_pause/seek/volume` command → 监听 `mpv-progress` 事件更新进度条/时间。

- [ ] **Step 2: 控制条/键盘/全屏对接 mpv command**

播放/暂停/快退/快进/seek/音量/CC/全屏全部改为发 Tauri command。全屏时通知后端 resize 视频层。

- [ ] **Step 3: ipc.ts 更新**

`playerOpen` 返回 `{duration, hasSubtitle}`；增 `playerPause/playerSeek/playerVolume/playerSetSubtitle`。

- [ ] **Step 4: tsc + 运行验证**

Run: `npx tsc --noEmit && echo clean`；`npm run tauri:dev` 真机点视频，确认秒开、控制条可用、进度更新。

- [ ] **Step 5: 提交**
```bash
git add src/views/PlayerView.ts src/lib/ipc.ts
git commit -m "feat(player): PlayerView 改为 libmpv 控制面板，去 video 标签

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>"
```

---

## 阶段 3：字幕 + 全屏 + 键盘

### Task 3.1: 字幕（内挂轨道切换 + 外挂挂载 + CC 开关）

**Files:**
- Modify: `src-tauri/src/player/mod.rs`、`src-tauri/src/player/mpv.rs`、`src/views/PlayerView.ts`

- [ ] **Step 1: 外挂字幕挂载**

player_open 时若存在 `subtitles/<分类>/<路径>/<标题>.ass`（复用现有 `library::paths::subtitle_abs_path` + resolve_subtitle 逻辑），调 mpv `sub-add`。

- [ ] **Step 2: CC 开关映射 sub-visibility**

前端 CC 按钮 → `player_set_subtitle(bool)` → mpv `set_property sub-visibility`。

- [ ] **Step 3: 运行验证字幕显示 + 提交**

真机确认外挂/内挂字幕显示、CC 开关有效。
```bash
git add -A && git commit -m "feat(player): mpv 原生字幕（外挂挂载/内挂轨道/CC 开关）

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>"
```

### Task 3.2: 全屏 resize 同步 + 键盘

**Files:**
- Modify: `src/views/PlayerView.ts`、`src-tauri/src/player/embed_macos.rs`

- [ ] **Step 1: 窗口 resize/全屏时同步视频层尺寸**

监听窗口 resize（现有 `onResized`）→ 调后端 resize 视频 NSView 铺满内容区。

- [ ] **Step 2: 键盘快捷键转 mpv command**

空格播放/暂停、方向键 seek±10、Esc 退全屏——改为发 mpv command。

- [ ] **Step 3: 运行验证 + 提交**

真机确认全屏视频铺满、退出全屏恢复、键盘有效。
```bash
git add -A && git commit -m "feat(player): 全屏视频层 resize 同步与键盘控制

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>"
```

---

## 阶段 4：清理旧链路 + 内置 mpv 分发

### Task 4.1: 删除转码/HTTP/libass 旧链路

> **Windows 边界注意**：本计划的 libmpv 嵌入是 macOS 专属（NSView）。若项目仍需 Windows 打包（现有 CI），删除转码/HTTP 链路会使 Windows 失去播放能力。删除前必须与用户确认 Windows 处置：(a) 放弃 Windows；(b) Windows 走 `#[cfg(windows)]` 保留旧链路、macOS 走 mpv（则不能全删，改为按平台条件编译）；(c) Windows 后续单独做 HWND 嵌入。**未确认前不执行本 Task 的删除。**

**Files:**
- Delete: `src-tauri/src/player/transcode.rs`、`src-tauri/src/player/httpserver.rs`、`src-tauri/src/player/ffmpeg_paths.rs`、`src/components/SubtitleRenderer.ts`、`public/libass/`、`src/assets/fonts` 下 CJK 兜底字体
- Modify: `src-tauri/src/player/mod.rs`（去掉 `pub mod transcode/httpserver/ffmpeg_paths`）、`src-tauri/src/lib.rs`（去掉 httpserver::start、HttpServerState）、`package.json`（去 libass-wasm 依赖）

- [ ] **Step 1: 逐一删除并清理引用**

删文件后，清理 mod.rs/lib.rs 里的模块声明、setup 里的 httpserver 启动、PlayerView/字幕相关 import。确认无残留引用。

- [ ] **Step 2: 移除 libass-wasm npm 依赖**

`package.json` 去 `libass-wasm`；`npm install` 更新 lock。

- [ ] **Step 3: 编译 + 测试 + 构建全绿**

Run: `cargo build ... || echo clean`；`cargo test`；`npx tsc --noEmit`；`npm run build`。全通过（删掉的模块测试一并移除）。

- [ ] **Step 4: 提交**
```bash
git add -A && git commit -m "refactor(player): 删除转码/HTTP server/libass 旧播放链路

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>"
```

### Task 4.2: 内置 libmpv 分发

**Files:**
- Create: `scripts/fetch-mpv.mjs`（替代 fetch-ffmpeg）
- Modify: `src-tauri/tauri.conf.json`（打包 libmpv 及依赖）、`package.json`（scripts）、`build.rs`、`.github/workflows/`

- [ ] **Step 1: 确定 libmpv 分发形态**

调研 macOS 下打包 libmpv 的方式：打包 `libmpv.2.dylib` 及其依赖 dylib（用 `otool -L` 列依赖、`install_name_tool` 改 rpath 到 app bundle 内），或用静态 libmpv 构建。选可行方案。

- [ ] **Step 2: fetch-mpv 脚本 + 打包配置**

写脚本获取 libmpv 及依赖放入 bundle；tauri.conf 配置随包分发；运行时用 bundle 内 libmpv（非 /opt/homebrew）。

- [ ] **Step 3: 从干净环境验证（无 homebrew mpv 依赖）**

验证 app 用的是内置 libmpv（临时改 rpath 或在无 homebrew 路径测试）。

- [ ] **Step 4: 提交**
```bash
git add -A && git commit -m "build(player): 内置 libmpv 及依赖分发，替代 ffmpeg sidecar

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>"
```

---

## 降级路径：C1（阶段 0 NO-GO 时）

若阶段 0 判定 C2 不可行，切换 C1（mpv 独立窗口 + IPC）：
- Rust `Command` 启动内置 mpv：`mpv <mkv> --input-ipc-server=<socket> --force-window`（mpv 弹自己的原生窗口）。
- Rust 通过 Unix socket 发 JSON IPC 命令（`{"command":["set_property","pause",true]}` 等）控制播放/暂停/seek/音量。
- 前端 PlayerView 变纯控制面板，发 Tauri command → Rust → IPC。
- 字幕/全屏/多音轨由 mpv 独立窗口原生处理。
- 无需 libmpv FFI、无需 NSView 嵌入、无需透明合成——避开 C2 全部高风险点，仍真秒开。代价：视频在独立窗口。
- C1 的详细计划在阶段 0 NO-GO 后另写（沿用阶段 2-4 的 command/清理/分发结构，播放核心换成子进程+IPC）。

## 最终验收

- [ ] 点未缓存 mkv → **1 秒内看到画面**（真秒开，零转码）。
- [ ] AC-3 音频正常出声、可切多音轨。
- [ ] seek 任意位置即时响应。
- [ ] 内挂/外挂字幕显示、CC 开关有效。
- [ ] 全屏铺满、退出恢复、键盘快捷键有效。
- [ ] 退出播放干净销毁 mpv、无残留进程/视图。
- [ ] `cargo build` clean、`cargo test` 全过、`npx tsc --noEmit` clean、`npm run build` 成功。
- [ ] 内置 libmpv 分发（不依赖 homebrew）。
