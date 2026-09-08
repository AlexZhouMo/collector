//! 视频播放：ffmpeg 按需转码 + 前端 <video> 播放。
pub mod transcode;

use std::sync::Mutex;

/// 当前转码会话（同一时刻只播一个视频）。Task 2 定义 TranscodeSession。
#[derive(Default)]
pub struct PlayerState(pub Mutex<Option<transcode::TranscodeSession>>);
