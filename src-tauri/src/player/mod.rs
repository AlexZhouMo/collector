pub mod mpv;

use crate::error::{AppError, AppResult};
use mpv::Player;
use std::sync::Mutex;

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
