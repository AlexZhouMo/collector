pub mod mpv;

#[cfg(target_os = "macos")]
pub mod embed_macos;

use crate::error::{AppError, AppResult};
use mpv::Player;
use std::sync::Mutex;

/// 嵌入子视图句柄：存 mpv 渲染 NSView 和父 NSView（WebView content view）的
/// 裸指针（用 usize 满足 Send + Sync）。退出时用于移除子视图。
#[derive(Default)]
pub struct EmbedState(pub Mutex<Option<EmbedHandle>>);

#[derive(Clone, Copy)]
pub struct EmbedHandle {
    pub mpv_view: usize,
    pub parent: usize,
}

/// 全局播放器单例。`Player` 由 `player_init` 惰性创建（Task 3.3 建好播放
/// 子窗口后传入其原生句柄），未初始化时其余 command 通过 `with` 返回 Invalid。
///
/// `Player` 内部持有 libmpv2 的 `Mpv`，后者已 `unsafe impl Send + Sync`，
/// 因此 `Mutex<Option<Player>>` 天然满足 Tauri managed state 的 Send + Sync 要求。
#[derive(Default)]
pub struct PlayerState(pub Mutex<Option<Player>>);

/// 在 `Player` 已初始化时执行 `f`；否则返回 Invalid 错误。
fn with<T>(s: &PlayerState, f: impl FnOnce(&Player) -> AppResult<T>) -> AppResult<T> {
    let g = s.0.lock().unwrap();
    let p = g
        .as_ref()
        .ok_or_else(|| AppError::Invalid("player not initialized".into()))?;
    f(p)
}

#[tauri::command]
pub fn player_load(
    state: tauri::State<PlayerState>,
    path: String,
    subtitle: Option<String>,
) -> AppResult<()> {
    with(&state, |p| p.load(&path, subtitle.as_deref()))
}

#[tauri::command]
pub fn player_pause(state: tauri::State<PlayerState>, paused: bool) -> AppResult<()> {
    with(&state, |p| p.set_pause(paused))
}

#[tauri::command]
pub fn player_seek(state: tauri::State<PlayerState>, secs: f64) -> AppResult<()> {
    with(&state, |p| p.seek(secs))
}

#[tauri::command]
pub fn player_seek_to(state: tauri::State<PlayerState>, secs: f64) -> AppResult<()> {
    with(&state, |p| p.seek_absolute(secs))
}

#[tauri::command]
pub fn player_volume(state: tauri::State<PlayerState>, vol: f64) -> AppResult<()> {
    with(&state, |p| p.set_volume(vol))
}

#[tauri::command]
pub fn player_progress(state: tauri::State<PlayerState>) -> AppResult<(f64, f64)> {
    with(&state, |p| Ok((p.position(), p.duration())))
}

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
