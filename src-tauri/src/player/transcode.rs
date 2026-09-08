//! 视频 remux：把 H.264 MKV 用 ffmpeg `-c copy` 无损转封装成临时 MP4，
//! 再由前端经 asset:// 协议用 <video> 播放。不重编码（秒级），不做实时转码流。
use crate::error::{AppError, AppResult};
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::path::PathBuf;
use std::process::Command;

/// 用 ffprobe 取视频时长（秒）。失败返回 Err。
pub fn probe_duration(path: &str) -> AppResult<f64> {
    let out = Command::new("ffprobe")
        .args([
            "-v", "error",
            "-show_entries", "format=duration",
            "-of", "default=noprint_wrappers=1:nokey=1",
            path,
        ])
        .output()
        .map_err(|e| AppError::Other(format!("ffprobe spawn: {e}")))?;
    if !out.status.success() {
        return Err(AppError::Other(format!(
            "ffprobe failed: {}",
            String::from_utf8_lossy(&out.stderr)
        )));
    }
    let text = String::from_utf8_lossy(&out.stdout);
    Ok(text.trim().parse().unwrap_or(0.0))
}

/// 源路径 → 临时 mp4 输出路径（系统临时目录 + 路径 hash，稳定可复用）。
pub fn temp_mp4_path(src: &str) -> PathBuf {
    let mut h = DefaultHasher::new();
    src.hash(&mut h);
    std::env::temp_dir().join(format!("collector_{:016x}.mp4", h.finish()))
}

/// 把源视频 remux（`-c copy` 无损换容器）成临时 mp4，返回 (临时mp4绝对路径, 时长秒)。
/// 同一源已 remux 过（临时文件存在）则直接复用，不重转。
/// 非 H.264 等 `-c copy` 不兼容 MP4 的编码会导致 ffmpeg 失败，返回 Err（前端提示不支持）。
pub fn remux(path: &str) -> AppResult<(String, f64)> {
    let duration = probe_duration(path)?;
    let out = temp_mp4_path(path);
    // 复用：临时文件已存在且非空则直接用
    if out.metadata().map(|m| m.len() > 0).unwrap_or(false) {
        return Ok((out.to_string_lossy().into_owned(), duration));
    }
    let status = Command::new("ffmpeg")
        .args([
            "-nostdin",
            "-i", path,
            // 视频无损保留（H.264 直接进 MP4）；音频转 AAC——WebView <video>
            // 不支持 AC-3/DTS 等，遇到非 AAC 音频整个媒体会解码失败（画面也黑）。
            // 转 AAC 很轻（音频码率低），不影响整体秒级 remux。
            "-c:v", "copy",
            "-c:a", "aac",
            "-b:a", "192k",
            "-movflags", "+faststart",
            "-y",
            &out.to_string_lossy(),
        ])
        .status()
        .map_err(|e| AppError::Other(format!("ffmpeg spawn: {e}")))?;
    if !status.success() {
        // remux 失败（多为编码/容器不兼容），清理可能的半成品
        let _ = std::fs::remove_file(&out);
        return Err(AppError::Other(
            "无法转封装该视频（可能编码不受支持，当前仅支持 H.264）".into(),
        ));
    }
    Ok((out.to_string_lossy().into_owned(), duration))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn temp_path_is_stable_and_mp4() {
        let a = temp_mp4_path("/movies/x.mkv");
        let b = temp_mp4_path("/movies/x.mkv");
        assert_eq!(a, b); // 同源稳定
        assert!(a.to_string_lossy().ends_with(".mp4"));
        assert_ne!(a, temp_mp4_path("/movies/y.mkv")); // 不同源不同
    }
}
