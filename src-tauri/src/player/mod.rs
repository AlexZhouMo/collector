//! 视频播放：H.264 MKV remux 成临时 MP4，前端经 asset:// 用 <video> 播放。
pub mod transcode;

use crate::error::AppResult;
use serde::Serialize;
use std::sync::Mutex;

/// 当前播放的临时 mp4 路径（同一时刻只播一个视频，退出时删该文件）。
#[derive(Default)]
pub struct PlayerState(pub Mutex<Option<String>>);

/// player_open 返回给前端：临时 mp4 绝对路径（前端 convertFileSrc 后给 <video>）+ 时长。
#[derive(Serialize)]
pub struct PlayerInfo {
    pub src: String,
    pub duration: f64,
}

/// 打开视频：remux 成临时 mp4（秒级，等完成），返回 {src, duration}。
/// 记录临时路径供退出清理。若已有旧临时文件（上一个视频）先删。
#[tauri::command]
pub fn player_open(state: tauri::State<PlayerState>, path: String) -> AppResult<PlayerInfo> {
    let (tmp, duration) = transcode::remux(&path)?;
    let mut guard = state.0.lock().unwrap();
    // 删上一个视频的临时文件（若与本次不同）
    if let Some(old) = guard.take() {
        if old != tmp {
            let _ = std::fs::remove_file(&old);
        }
    }
    *guard = Some(tmp.clone());
    Ok(PlayerInfo { src: tmp, duration })
}

/// 停止播放：删当前临时 mp4，清状态（退出播放模式时调用）。
#[tauri::command]
pub fn player_stop(state: tauri::State<PlayerState>) -> AppResult<()> {
    if let Some(tmp) = state.0.lock().unwrap().take() {
        let _ = std::fs::remove_file(&tmp);
    }
    Ok(())
}
