# 视频嵌入式播放 + 大文件流畅退出 设计文档

- 日期：2026-09-07
- 状态：设计已确认，待复审

## Context

当前播放器：点视频后，后端 `open_player_window` 创建一个**独立无边框窗口**承载 mpv 渲染，主窗口只显示浮层控制条（`.player-bar` fixed 在底部）。问题：

1. **视频不是"嵌入"内容面板**：它在一个独立窗口播放，不是嵌在右侧内容区里。
2. **退出卡死的根源**：`PlayerView.cleanup()` 只 `clearInterval` + 存进度，**从不销毁 mpv**——Player 实例（持有 libmpv 的 Mpv）和 mpv 窗口一直残留在后台。没有 stop/销毁 command。大文件反复播放时资源累积，退出时体感卡死。
3. mpv 窗口位置固定（960×540 无边框），不随主窗内容区变化。

用户要求：视频嵌入右侧内容面板播放，下方常驻控制条（播放/暂停、快进快退、音量、进度、全屏）；大容量视频流畅不卡顿，尤其**退出不卡死**。

## 已确认的技术决策

| 决策点 | 结论 |
|---|---|
| 嵌入方式 | **定位覆盖**：mpv 仍是独立原生窗口，但 Rust 实时把它的位置/大小对齐到主窗内容区里"视频区"矩形，视觉上嵌入。保留 mpv 原生渲染 + 硬解（`hwdec=auto`），流畅度最佳。 |
| 退出清理 | **彻底销毁**：退出播放模式时停播放、关 mpv 窗口、drop 掉 PlayerState 里的 Player 实例。 |
| 销毁时机 | **先 stop 再异步销毁**：退出先发 `stop` 命令停播放/卸载文件（瞬时返回），UI 立即切回列表；Player 实例的 drop（可能耗时）放到不阻塞前台的路径。 |
| 控制条 | 常驻显示在视频区下方（内容面板内），不自动隐藏。 |
| 布局 | 内容面板播放模式：顶部返回+标题，中间视频区矩形，底部玻璃控制条。左侧菜单保留。 |

## 架构与改动

### 后端 `src-tauri/src/player/mpv.rs`
- 新增 `stop()`：发 mpv `stop` 命令（停止播放并卸载当前文件，瞬时）。
- 保留现有 load/pause/seek/volume/fullscreen/position/duration。

### 后端 `src-tauri/src/player/mod.rs`
- 新增 command `player_stop`：调 `Player::stop()`。
- 新增 command `player_close`：停止播放 + 把 `PlayerState` 的 `Option<Player>` 置 `None`（drop 掉 Mpv，释放 libmpv 资源）。drop 在锁内发生；为避免阻塞，先在 `player_stop` 里停播放，`player_close` 只做 take + drop（此时文件已卸载，drop 快）。

### 后端 `src-tauri/src/lib.rs`
- `open_player_window`：改造为可传入目标矩形（x/y/宽/高），创建/复用 mpv 窗口后用 `set_position`/`set_size` 贴合内容区矩形。窗口保持无边框、无标题栏。
- 新增 command `player_set_bounds(x, y, width, height)`：更新 mpv 窗口位置尺寸（前端在内容区尺寸变化/窗口 resize 时调用，保持覆盖对齐）。
- 新增 command `player_close_window`：隐藏或关闭 mpv 窗口（配合 player_close）。
- 退出流程：前端先 `player_stop` → `player_close_window`（藏窗口，UI 立即恢复）→ `player_close`（drop 实例）。

### 前端 `src/views/PlayerView.ts`（重构为嵌入式）
- 结构改为内容面板内的三段式：顶部返回+标题、中间 `.player-stage`（视频区占位矩形，mpv 覆盖它）、底部 `.player-bar` 控制条（改为内容流内、非 fixed）。
- 挂载后测量 `.player-stage` 的屏幕坐标（getBoundingClientRect + 窗口偏移），调 `openPlayerWindow` 传该矩形；用 `ResizeObserver` 监听 stage 尺寸变化，变化时调 `player_set_bounds` 保持对齐。
- 控制条：播放/暂停、快退10s/快进10s、进度条 seek、时间、音量、全屏，逻辑复用现有。
- **退出（cleanup）**：`clearInterval` → 存最终进度 → `player_stop` → `player_close_window`（藏窗口，onExit 立即切回列表）→ `player_close`（异步 drop，不 await 阻塞 UI）。保证退出瞬时、不卡死。
- 进度轮询保持 1s；seeking 抑制回拉保持。

### 前端 `src/lib/ipc.ts`
- 新增 `playerStop()`、`playerClose()`、`playerSetBounds(x,y,w,h)`、`playerCloseWindow()`。
- `openPlayerWindow` 改为接收矩形参数 `openPlayerWindow(x,y,w,h)`。

### 坐标对齐要点
- mpv 窗口是 OS 屏幕坐标；`.player-stage` 是 DOM 坐标。需要主窗口在屏幕上的位置（Tauri `window.outer_position` / `inner_position`）+ stage 相对主窗的 rect，换算出 mpv 窗口的屏幕坐标。前端拿主窗位置（Tauri API `getCurrentWindow().outerPosition()`）+ stage.getBoundingClientRect() 合成，传给 `player_set_bounds`。
- 主窗移动/缩放时也要同步：监听 Tauri 窗口 move/resize 事件重新对齐（可选增强，先保证 stage ResizeObserver + 初次定位）。

## 流畅度优化（大文件不卡顿）

- 保留 `hwdec=auto`（硬件解码），这是大文件流畅的关键，已在。
- mpv 原生渲染路径（非纹理/软合成），避免前端解码瓶颈。
- 退出彻底销毁，杜绝多实例资源累积导致的后续卡顿。
- 续播 seek 仍延迟到文件加载后（现有 300ms 逻辑保留）。

## 明确不做（YAGNI）

- 不做倍速、不做控制条自动隐藏（常驻）。
- 不做真·子视图嵌入（addSubview）或纹理合成——定位覆盖已满足需求且更稳。
- 主窗移动时的实时跟随作为可选增强，不阻塞核心（先保证初次定位 + stage resize 对齐）。

## 已知技术风险（需真机验证）

- **定位覆盖的层级/焦点**：mpv 是叠在主窗之上的独立窗口，其 z-order（始终在主窗内容区之上、但不遮挡左菜单和控制条）、焦点抢占、以及主窗被其他应用窗口盖住时 mpv 窗口是否跟随隐藏，都是 macOS 窗口管理的运行时行为，需真机验证。若层级/跟随有问题，回退手段：把 mpv 窗口设为主窗的**子窗口**（child window，随父窗移动/层级绑定），Tauri 支持设置 parent。
- **坐标换算精度**：DPI 缩放（Retina）下 DOM 逻辑像素与屏幕物理像素的换算需用主窗 scaleFactor 校正，否则覆盖矩形会偏移。实现时用 Tauri 的 `scaleFactor()` 校正。
- 这些与既有"mpv 嵌入需真机验证"同源；实现阶段先跑通再按真机表现微调，回退路径（child window）已备。

## 验证

1. 后端 `cargo test` + `cargo build` 通过。
2. 前端 `npm run build` 通过。
3. 应用运行：点视频 → 内容面板切换为播放模式，视频画面出现在内容区矩形内（对齐），下方控制条可播放/暂停/快进快退/调音量/拖进度/全屏。
4. **退出验证**：点返回 → UI 立即切回视频列表（不卡顿/不卡死），mpv 窗口消失，Player 实例释放。反复进出多个大文件，无累积卡顿。
5. 缩放主窗口 → 视频区矩形随之变化，mpv 覆盖保持对齐（stage ResizeObserver 生效）。
