# 视频嵌入式播放 + 大文件流畅退出 实现计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 把视频播放从"独立 mpv 窗口 + 浮层控制条"改为"mpv 窗口定位覆盖内容面板的视频区（嵌入观感）+ 内容流内控制条"，并彻底修复退出不清理 mpv 导致的卡死。

**Architecture:** mpv 仍渲染到独立无边框原生窗口，但设为主窗的**子窗口**（set_parent，层级/移动跟随父窗），Rust 按前端传入的屏幕矩形 set_position/set_size 使其精确覆盖内容面板里的 `.player-stage`。退出时三步清理：stop 停播卸载文件 → 隐藏 mpv 窗口 + UI 立即切回 → drop Player 实例释放 libmpv。

**Tech Stack:** Tauri 2.x（WebviewWindow set_position/set_size/set_parent/hide/show）、libmpv2、前端 @tauri-apps/api/window（outerPosition/scaleFactor/onMoved）、ResizeObserver。

**参考设计：** `docs/superpowers/specs/2026-09-07-embedded-player-design.md`

**当前代码事实：**
- `open_player_window`（lib.rs）建无边框 "mpv" 窗口，取 ns_view/hwnd 传 `Player::new(wid)`；固定 960×540。
- `player/mpv.rs` 有 load/pause/seek/seek_absolute/volume/fullscreen/position/duration，**无 stop**。
- `player/mod.rs` 有 player_load/pause/seek/seek_to/volume/progress，**无 stop/close**。PlayerState = `Mutex<Option<Player>>`。
- `PlayerView.ts` 用浮层 `.player-bar`（fixed），cleanup 只 clearInterval+存进度，**不销毁 mpv**。
- 图标模块 `src/lib/icons.ts`（icon()），播放相关图标已有：play/pause/rewind/forward/fullscreen/arrowLeft。

---

## 文件结构

- `src-tauri/src/player/mpv.rs` — 加 `stop()`
- `src-tauri/src/player/mod.rs` — 加 `player_stop`、`player_close` command
- `src-tauri/src/lib.rs` — 改造 `open_player_window`（传矩形+set_parent）、加 `player_set_bounds`、`player_close_window`
- `src/lib/ipc.ts` — 加 playerStop/playerClose/playerSetBounds/playerCloseWindow，改 openPlayerWindow 签名
- `src/views/PlayerView.ts` — 重构为嵌入式（stage 矩形 + 内容流控制条 + ResizeObserver + 三步退出）
- `src/styles/theme.css` — 播放模式布局样式（.player-view/.player-stage/.player-bar 改为流内）

---

## Task 1: mpv stop 方法

**Files:** Modify `src-tauri/src/player/mpv.rs`

- [ ] **Step 1: 加 stop 方法**

在 `impl Player` 内（`set_fullscreen` 之后）加：
```rust
    /// 停止播放并卸载当前文件。用于退出播放前的瞬时停止，
    /// 使后续 drop Player 时 libmpv 无需卸载大文件解码器，避免阻塞。
    pub fn stop(&self) -> AppResult<()> {
        self.mpv
            .command("stop", &[])
            .map_err(|e| AppError::Other(format!("stop: {e:?}")))
    }
```

- [ ] **Step 2: 编译**

Run: `cd src-tauri && cargo build 2>&1 | tail -3`
Expected: 编译通过（Player::stop 暂未被调用，normalize 无 allow(dead_code)，此处会有 1 条 dead_code 警告——下个任务 command 调用后消失，可接受）。

- [ ] **Step 3: 提交**

```bash
git add src-tauri/src/player/mpv.rs
git commit -m "feat: add mpv stop to unload file before teardown"
```

## Task 2: player_stop / player_close command

**Files:** Modify `src-tauri/src/player/mod.rs`

- [ ] **Step 1: 加两个 command**

在 `player_progress` 之后追加：
```rust
/// 停止播放并卸载当前文件（瞬时）。退出播放模式的第一步。
#[tauri::command]
pub fn player_stop(state: tauri::State<PlayerState>) -> AppResult<()> {
    // 未初始化时视为无操作（已经是停止态）
    let g = state.0.lock().unwrap();
    if let Some(p) = g.as_ref() {
        p.stop()?;
    }
    Ok(())
}

/// 释放 Player 实例（drop libmpv）。在 player_stop 之后调用——此时文件已
/// 卸载，drop 很快。take 出 Option 后锁外 drop，尽量减少持锁时间。
#[tauri::command]
pub fn player_close(state: tauri::State<PlayerState>) -> AppResult<()> {
    let taken = state.0.lock().unwrap().take();
    drop(taken); // 显式在锁释放后 drop
    Ok(())
}
```

- [ ] **Step 2: 注册 command** — 在 `src-tauri/src/lib.rs` 的 `generate_handler!` 里，`player::player_progress` 之后追加 `player::player_stop, player::player_close`。

- [ ] **Step 3: 编译**

Run: `cd src-tauri && cargo build 2>&1 | tail -3`
Expected: 编译通过（Player::stop 现在被 player_stop 用，dead_code 警告消失）。

- [ ] **Step 4: 提交**

```bash
git add src-tauri/src/player/mod.rs src-tauri/src/lib.rs
git commit -m "feat: player_stop and player_close commands"
```

## Task 3: 改造 open_player_window（矩形 + 子窗口）+ set_bounds + close_window

**Files:** Modify `src-tauri/src/lib.rs`

- [ ] **Step 1: 改 open_player_window 接收矩形并设为子窗口**

把 `open_player_window` 整个函数替换为（加 `rename_all = "camelCase"`，参数含下划线；新建时用传入矩形，并 set_parent 主窗）：
```rust
/// 创建/复用承载 mpv 渲染的子窗口，定位到内容面板视频区矩形（屏幕坐标）。
/// 设为主窗子窗口，使层级与移动跟随父窗。
#[tauri::command(rename_all = "camelCase")]
fn open_player_window(
    app: tauri::AppHandle,
    state: tauri::State<player::PlayerState>,
    x: f64,
    y: f64,
    width: f64,
    height: f64,
) -> AppResult<()> {
    use tauri::{LogicalPosition, LogicalSize, Manager, WebviewWindowBuilder};
    let win = match app.get_webview_window("mpv") {
        Some(w) => w,
        None => {
            let w = WebviewWindowBuilder::new(
                &app,
                "mpv",
                tauri::WebviewUrl::App("about:blank".into()),
            )
            .title("player")
            .decorations(false)
            .inner_size(width.max(1.0), height.max(1.0))
            .build()
            .map_err(|e| error::AppError::Other(e.to_string()))?;
            // 绑定为主窗子窗口（层级/移动跟随）。主窗 label 是默认 "main"。
            if let Some(main) = app.get_webview_window("main") {
                let _ = w.set_parent(&main);
            }
            w
        }
    };
    win.set_position(LogicalPosition::new(x, y))
        .map_err(|e| error::AppError::Other(e.to_string()))?;
    win.set_size(LogicalSize::new(width.max(1.0), height.max(1.0)))
        .map_err(|e| error::AppError::Other(e.to_string()))?;
    let _ = win.show();
    #[cfg(target_os = "macos")]
    let wid = win
        .ns_view()
        .map_err(|e| error::AppError::Other(e.to_string()))? as i64;
    #[cfg(target_os = "windows")]
    let wid = win
        .hwnd()
        .map_err(|e| error::AppError::Other(e.to_string()))?
        .0 as i64;
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    let wid: i64 = 0;
    // 仅在尚未初始化时创建 Player（复用窗口时不重建）
    let mut guard = state.0.lock().unwrap();
    if guard.is_none() {
        *guard = Some(player::mpv::Player::new(wid)?);
    }
    Ok(())
}
```
注：`LogicalPosition`/`LogicalSize` 让 Tauri 按 DPI 自动换算，前端传逻辑坐标（DOM 像素）即可，无需手动乘 scaleFactor。主窗 label 通常是 "main"；若实际不同，用 `app.webview_windows()` 取第一个非 mpv 窗口兜底——为稳妥，写：`app.get_webview_window("main").or_else(|| app.webview_windows().into_iter().find(|(l,_)| l != "mpv").map(|(_,w)| w))`。实现时若 "main" 存在则直接用。

- [ ] **Step 2: 加 player_set_bounds 和 player_close_window**

在 `open_player_window` 之后加：
```rust
/// 更新 mpv 窗口位置/尺寸（前端在内容区 resize / 主窗移动时调用保持覆盖）。
#[tauri::command(rename_all = "camelCase")]
fn player_set_bounds(
    app: tauri::AppHandle,
    x: f64,
    y: f64,
    width: f64,
    height: f64,
) -> AppResult<()> {
    use tauri::{LogicalPosition, LogicalSize, Manager};
    if let Some(win) = app.get_webview_window("mpv") {
        win.set_position(LogicalPosition::new(x, y))
            .map_err(|e| error::AppError::Other(e.to_string()))?;
        win.set_size(LogicalSize::new(width.max(1.0), height.max(1.0)))
            .map_err(|e| error::AppError::Other(e.to_string()))?;
    }
    Ok(())
}

/// 隐藏 mpv 窗口（退出播放时立即调用，UI 观感上瞬时消失）。
#[tauri::command]
fn player_close_window(app: tauri::AppHandle) -> AppResult<()> {
    use tauri::Manager;
    if let Some(win) = app.get_webview_window("mpv") {
        let _ = win.hide();
    }
    Ok(())
}
```

- [ ] **Step 3: 注册** — `generate_handler!` 里 `open_player_window` 之后追加 `player_set_bounds, player_close_window`。

- [ ] **Step 4: 编译**

Run: `cd src-tauri && cargo build 2>&1 | tail -5`
Expected: 编译通过。若 `set_parent` 在当前 Tauri 版本 API 名不同（如 `set_parent_window`），以实际编译为准调整；若该平台不支持则去掉 set_parent（定位覆盖仍工作，仅层级跟随弱化），并在提交信息注明。

- [ ] **Step 5: 提交**

```bash
git add src-tauri/src/lib.rs
git commit -m "feat: embed mpv window via bounds + child-window binding"
```

## Task 4: 前端 IPC 扩展

**Files:** Modify `src/lib/ipc.ts`

- [ ] **Step 1: 改 openPlayerWindow 签名 + 加 4 个方法**

找到 `openPlayerWindow: () => invoke<void>("open_player_window"),`，替换为：
```ts
  openPlayerWindow: (x: number, y: number, width: number, height: number) =>
    invoke<void>("open_player_window", { x, y, width, height }),
  playerSetBounds: (x: number, y: number, width: number, height: number) =>
    invoke<void>("player_set_bounds", { x, y, width, height }),
  playerStop: () => invoke<void>("player_stop"),
  playerClose: () => invoke<void>("player_close"),
  playerCloseWindow: () => invoke<void>("player_close_window"),
```

- [ ] **Step 2: 编译**

Run: `npm run build 2>&1 | tail -6`
Expected: tsc 报 PlayerView.ts 里 `openPlayerWindow()` 调用参数不匹配（下个任务修）。**这是预期的**——本任务只改 ipc，PlayerView 在 Task 5 更新。为让本任务可独立提交且编译通过，可先跳过 build 验证，直接进 Task 5，或本任务与 Task 5 合并提交。

**决策：本任务不单独 build/commit，与 Task 5 一起提交**（因为改了签名必然破坏 PlayerView，两者是同一处契约变更）。跳到 Task 5。

## Task 5: PlayerView 重构为嵌入式 + 三步退出

**Files:** Modify `src/views/PlayerView.ts`, `src/styles/theme.css`

- [ ] **Step 1: 重写 PlayerView.ts**

整个文件替换为：
```ts
import { api } from "../lib/ipc";
import type { MediaItem } from "../lib/ipc";
import { icon } from "../lib/icons";
import { getCurrentWindow } from "@tauri-apps/api/window";

export async function PlayerView(it: MediaItem, onExit: () => void): Promise<HTMLElement> {
  const el = document.createElement("div");
  el.className = "player-view view-enter";
  el.innerHTML = `
    <div class="player-top">
      <button class="back">${icon("arrowLeft", 16)}<span>返回</span></button>
      <span class="player-title">${it.title.replace(/[<&]/g, "")}</span>
    </div>
    <div class="player-stage"></div>
    <div class="player-bar glass">
      <button class="rw">${icon("rewind", 18)}</button>
      <button class="pp">${icon("pause", 18)}</button>
      <button class="ff">${icon("forward", 18)}</button>
      <input class="seek" type="range" min="0" max="1000" value="0"/>
      <span class="time">0:00 / 0:00</span>
      <input class="vol" type="range" min="0" max="100" value="100"/>
      <button class="fs">${icon("fullscreen", 18)}</button>
    </div>`;

  const stage = el.querySelector<HTMLElement>(".player-stage")!;
  const seek = el.querySelector<HTMLInputElement>(".seek")!;
  const time = el.querySelector<HTMLElement>(".time")!;
  const fmt = (s: number) => `${Math.floor(s / 60)}:${String(Math.floor(s % 60)).padStart(2, "0")}`;
  let paused = false, fullscreen = false, seeking = false, lastPos = 0, closed = false;

  // 计算 stage 在屏幕上的逻辑坐标（主窗外框位置 + stage 相对视口的 rect）
  const appWin = getCurrentWindow();
  async function stageBounds() {
    const r = stage.getBoundingClientRect();
    const pos = await appWin.outerPosition(); // 物理像素
    const sf = await appWin.scaleFactor();
    // outerPosition 是物理像素，转逻辑；DOM rect 是逻辑像素。
    const originX = pos.x / sf, originY = pos.y / sf;
    // 主窗外框左上到视口左上有装饰高度差；用 innerPosition 更准
    const inner = await appWin.innerPosition();
    const ix = inner.x / sf, iy = inner.y / sf;
    return { x: ix + r.left, y: iy + r.top, width: r.width, height: r.height };
  }

  async function positionMpv() {
    const b = await stageBounds();
    await api.playerSetBounds(b.x, b.y, b.width, b.height);
  }

  // 初始：先按 stage 矩形创建/定位 mpv 窗口，再 load
  const b0 = await stageBounds();
  await api.openPlayerWindow(b0.x, b0.y, b0.width, b0.height);
  await api.playerLoad(it.path, it.subtitle_path);
  api.getVideoPos(it.id).then((resume) => {
    if (resume > 5) setTimeout(() => api.playerSeekTo(resume), 300);
  }).catch(() => {});

  // stage 尺寸变化时保持覆盖对齐
  const ro = new ResizeObserver(() => { if (!closed) positionMpv().catch(() => {}); });
  ro.observe(stage);
  // 主窗移动时也对齐
  const unlistenMovedP = appWin.onMoved(() => { if (!closed) positionMpv().catch(() => {}); });

  el.querySelector<HTMLButtonElement>(".pp")!.onclick = async () => {
    paused = !paused;
    await api.playerPause(paused);
    el.querySelector(".pp")!.innerHTML = icon(paused ? "play" : "pause", 18);
  };
  el.querySelector<HTMLButtonElement>(".rw")!.onclick = () => api.playerSeek(-10);
  el.querySelector<HTMLButtonElement>(".ff")!.onclick = () => api.playerSeek(10);
  el.querySelector<HTMLInputElement>(".vol")!.oninput = (e) =>
    api.playerVolume(Number((e.target as HTMLInputElement).value));
  seek.oninput = () => { seeking = true; };
  seek.onchange = async () => {
    const [, dur] = await api.playerProgress();
    api.playerSeekTo((Number(seek.value) / 1000) * dur);
    setTimeout(() => { seeking = false; }, 600);
  };
  el.querySelector<HTMLButtonElement>(".fs")!.onclick = async () => {
    fullscreen = !fullscreen;
    await api.playerFullscreen(fullscreen);
  };

  const timer = setInterval(async () => {
    if (seeking || closed) return;
    try {
      const [pos, dur] = await api.playerProgress();
      if (dur > 0) {
        seek.value = String((pos / dur) * 1000);
        time.textContent = `${fmt(pos)} / ${fmt(dur)}`;
      }
      lastPos = pos;
      api.setVideoPos(it.id, pos).catch(() => {});
    } catch {}
  }, 1000);

  // 三步退出：停播卸载 → 藏窗口+UI切回 → 异步 drop 实例（不阻塞前台）
  const cleanup = async () => {
    if (closed) return;
    closed = true;
    clearInterval(timer);
    ro.disconnect();
    unlistenMovedP.then((un) => un()).catch(() => {});
    if (lastPos > 0) api.setVideoPos(it.id, lastPos).catch(() => {});
    await api.playerStop().catch(() => {});         // 停播卸载文件（瞬时）
    await api.playerCloseWindow().catch(() => {});   // 藏 mpv 窗口
    api.playerClose().catch(() => {});               // 异步 drop 实例，不 await
  };
  el.querySelector<HTMLButtonElement>(".back")!.onclick = async () => {
    await cleanup();
    onExit();
  };
  el.addEventListener("player-detach", () => { cleanup(); });
  return el;
}
```
注意：`it.title.replace(/[<&]/g,"")` 是简单去除破坏 HTML 的字符（标题来自本地扫描，风险低；这里不引 esc 以保持文件独立，若项目要求统一可 import esc）。实际实现时若 esc 已在别处统一用，改 `import { esc } from "../lib/escape"` 并用 `esc(it.title)`。

- [ ] **Step 2: 改 main.ts 的 openPlayer（player-detach 现在是异步 cleanup）**

`src/main.ts` 的 `openPlayer` 里 `prev?.dispatchEvent(new Event("player-detach"))` 保持不变（cleanup 内部自处理 closed 幂等）。无需改。

- [ ] **Step 3: 播放模式布局 CSS**

`src/styles/theme.css` 里找到旧的 `.player-bar{position:fixed;...}` 那条，替换为流内布局，并加 player-view/stage/top 样式。把：
```css
.player-bar{position:fixed;left:50%;bottom:24px;transform:translateX(-50%);display:flex;align-items:center;gap:10px;padding:10px 16px;z-index:10}
```
替换为：
```css
.player-view{height:100%;display:flex;flex-direction:column;gap:10px}
.player-top{display:flex;align-items:center;gap:12px}
.player-title{color:var(--text);font-size:14px}
.player-stage{flex:1;min-height:0;border-radius:10px;background:#000;border:1px solid var(--border)}
.player-bar{display:flex;align-items:center;gap:12px;padding:9px 14px}
```
`.player-bar .seek{...}` 和 `.player-bar button{...}` 两条保留不动（宽度/透明按钮样式仍适用）。

- [ ] **Step 4: 编译验证**

Run: `npm run build 2>&1 | tail -6` — tsc+vite 通过。
Run: `cd src-tauri && cargo build 2>&1 | tail -3` — 通过。

- [ ] **Step 5: 提交**

```bash
git add src/lib/ipc.ts src/views/PlayerView.ts src/styles/theme.css
git commit -m "feat: embedded video playback in content panel with clean exit"
```

## Task 6: 运行时验证（真机）

**Files:** 无（验证 + 按需微调）

- [ ] **Step 1: 运行应用**

应用 dev 模式运行中会热重载；若未运行，`npm run tauri dev`（后台 nohup 方式）。

- [ ] **Step 2: 手动验证清单**

1. 点视频 → 内容面板切为播放模式，mpv 画面出现在 `.player-stage` 矩形内、对齐（不遮挡左菜单/控制条/标题）。
2. 控制条：播放/暂停切换、快退/快进 10s、拖进度条 seek、音量、全屏切换均生效。
3. **退出**：点返回 → UI 立即切回视频列表（不卡顿/卡死），mpv 画面消失。反复进出多个大文件，无累积卡顿、无卡死。
4. 缩放主窗口 → 视频区矩形随之变化，mpv 覆盖跟随对齐（ResizeObserver + onMoved）。
5. 续播：中途退出再进同一视频，从上次位置继续。

- [ ] **Step 3: 按真机表现微调**（若需要）

- 若 mpv 覆盖位置有偏移（DPI/装饰高度）：核对 `stageBounds` 用 innerPosition + scaleFactor 的换算，调整。
- 若 mpv 窗口层级不对（被主窗盖住或盖住控制条）：确认 set_parent 生效；若无效，可尝试 `set_always_on_top` 临时置顶或调整父子绑定。
- 若退出仍有短暂卡顿：确认 player_stop 在 player_close 之前、且 player_close 未被 await 阻塞 UI。

- [ ] **Step 4: 记录验证结果**（提交一条 docs 或在 PR 描述里写明真机表现，无代码改动则跳过提交）

---

## 自查

**1. Spec 覆盖：**
- 视频嵌入内容面板（定位覆盖）→ Task 3（open_player_window 矩形+子窗）+ Task 5（stage + positionMpv）✓
- 下方控制条（播放/暂停/快进快退/音量/进度/全屏）→ Task 5 控制条 ✓
- 大文件流畅（hwdec 保留、原生渲染）→ 沿用现有 mpv.rs 的 hwdec=auto，未改动 ✓
- 退出不卡死（三步清理）→ Task 1（stop）+ Task 2（player_stop/close）+ Task 3（close_window）+ Task 5（cleanup 三步）✓
- 缩放对齐 → Task 5 ResizeObserver + onMoved ✓
- 续播保留 → Task 5 getVideoPos/setVideoPos ✓

**2. 占位符扫描：** 无 TBD/TODO。Task 3 Step 4、Task 6 Step 3 标注的"以实际编译为准/按真机微调"是真实工程不确定性（Tauri API 名、原生窗口运行时行为），已给明确的判断标准和回退，非占位。

**3. 类型一致性：** 后端 command 名（player_stop/player_close/player_set_bounds/player_close_window/open_player_window）与前端 ipc（playerStop/playerClose/playerSetBounds/playerCloseWindow/openPlayerWindow）一一对应；含下划线参数的 command 用 rename_all camelCase，前端传 camelCase（x/y/width/height 无下划线，无需转换）。openPlayerWindow 签名前后端一致（4 参数）。

自查通过。
