//! 视频播放：H.264 MKV remux 成 MP4（放应用数据缓存目录，asset:// 可访问），
//! 前端经 asset:// 用 <video> 播放。缓存产物不随停止删除，磁盘由 LRU 管理。
pub mod transcode;

use crate::error::AppResult;
use serde::Serialize;
use std::sync::Mutex;
use tauri::Manager;

/// 当前播放的 mp4 缓存路径（同一时刻只播一个视频）。
#[derive(Default)]
pub struct PlayerState(pub Mutex<Option<String>>);

/// player_open 返回给前端：mp4 绝对路径（前端 convertFileSrc 后给 <video>）+ 时长。
#[derive(Serialize)]
pub struct PlayerInfo {
    pub src: String,
    pub duration: f64,
}

/// 打开视频：remux 成缓存目录下 mp4（秒级，等完成），返回 {src, duration}。
/// 缓存目录取 <app_data_dir>/video_cache（asset 协议可访问）。
#[tauri::command]
pub fn player_open(
    app: tauri::AppHandle,
    state: tauri::State<PlayerState>,
    path: String,
) -> AppResult<PlayerInfo> {
    let cache_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| crate::error::AppError::Other(format!("app_data_dir: {e}")))?
        .join("video_cache");
    let (src, duration) = transcode::remux(&cache_dir, &path)?;
    *state.0.lock().unwrap() = Some(src.clone());
    Ok(PlayerInfo { src, duration })
}

/// 停止播放：清状态（退出播放模式时调用）。不删缓存产物，磁盘由 LRU 管。
#[tauri::command]
pub fn player_stop(state: tauri::State<PlayerState>) -> AppResult<()> {
    *state.0.lock().unwrap() = None;
    Ok(())
}
