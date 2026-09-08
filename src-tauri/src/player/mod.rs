//! 视频播放：ffmpeg 按需转码 + 前端 <video> 播放。
pub mod transcode;

use std::sync::Mutex;

/// 当前转码会话（同一时刻只播一个视频）。Task 2 定义 TranscodeSession。
#[derive(Default)]
pub struct PlayerState(pub Mutex<Option<transcode::TranscodeSession>>);

use crate::error::AppResult;
use transcode::TranscodeSession;

/// 打开视频：起一个从头开始的转码会话，返回时长（秒，供前端进度条）。
#[tauri::command]
pub fn player_open(state: tauri::State<PlayerState>, path: String) -> AppResult<f64> {
    let session = TranscodeSession::start(&path, 0.0)?;
    let dur = session.duration_secs;
    let mut guard = state.0.lock().unwrap();
    *guard = Some(session); // 旧会话（若有）在此被替换，其 Drop 会 kill 旧 ffmpeg
    Ok(dur)
}

/// seek：从 secs 起重开转码会话。前端随后重载 <video>。
#[tauri::command(rename_all = "camelCase")]
pub fn player_seek(state: tauri::State<PlayerState>, path: String, secs: f64) -> AppResult<()> {
    let session = TranscodeSession::start(&path, secs)?;
    let mut guard = state.0.lock().unwrap();
    *guard = Some(session);
    Ok(())
}

/// 停止播放：kill 会话（退出播放模式时调用）。
#[tauri::command]
pub fn player_stop(state: tauri::State<PlayerState>) -> AppResult<()> {
    *state.0.lock().unwrap() = None; // take+drop → kill ffmpeg
    Ok(())
}
