//! ffmpeg 按需转码会话：探测源编码，起 ffmpeg 输出 fMP4 到 stdout，
//! 供自定义协议流式读取。H.264 直接 remux，其它编码 VideoToolbox 硬编转 H.264。
use crate::error::{AppError, AppResult};
use std::io::Read;
use std::process::{Child, Command, Stdio};

/// 源视频探测结果。
#[derive(Debug, Clone)]
pub struct Probe {
    pub video_is_h264: bool,
    pub audio_is_aac: bool,
    pub duration_secs: f64,
}

/// 用 ffprobe 探测视频/音频编码与时长。
pub fn probe(path: &str) -> AppResult<Probe> {
    let out = Command::new("ffprobe")
        .args([
            "-v", "error",
            "-show_entries", "stream=codec_type,codec_name",
            "-show_entries", "format=duration",
            "-of", "default=noprint_wrappers=1",
            path,
        ])
        .output()
        .map_err(|e| AppError::Other(format!("ffprobe spawn: {e}")))?;
    // 校验退出码：文件不存在/非视频/损坏时 ffprobe 非 0 退出。不校验会静默
    // 返回全 false 的 Probe、走错转码分支、最终变成难查的"空流"。
    if !out.status.success() {
        return Err(AppError::Other(format!(
            "ffprobe failed: {}",
            String::from_utf8_lossy(&out.stderr)
        )));
    }
    let text = String::from_utf8_lossy(&out.stdout);
    let mut video_is_h264 = false;
    let mut audio_is_aac = false;
    let mut duration_secs = 0.0;
    let mut cur_type = String::new();
    for line in text.lines() {
        if let Some(v) = line.strip_prefix("codec_type=") {
            cur_type = v.trim().to_string();
        } else if let Some(v) = line.strip_prefix("codec_name=") {
            let name = v.trim();
            if cur_type == "video" && name == "h264" {
                video_is_h264 = true;
            }
            if cur_type == "audio" && name == "aac" {
                audio_is_aac = true;
            }
        } else if let Some(v) = line.strip_prefix("duration=") {
            duration_secs = v.trim().parse().unwrap_or(0.0);
        }
    }
    Ok(Probe { video_is_h264, audio_is_aac, duration_secs })
}

/// 构造 ffmpeg 参数：按需选择 copy / videotoolbox 硬编。
/// start_secs > 0 时从该位置起转（seek）。输出 fMP4 到 stdout(pipe:1)。
pub fn build_args(path: &str, probe: &Probe, start_secs: f64) -> Vec<String> {
    let mut args: Vec<String> = Vec::new();
    args.push("-nostdin".into());
    if start_secs > 0.0 {
        args.push("-ss".into());
        args.push(format!("{start_secs}"));
    }
    args.push("-i".into());
    args.push(path.into());
    args.push("-c:v".into());
    if probe.video_is_h264 {
        args.push("copy".into());
    } else {
        args.push("h264_videotoolbox".into());
        args.push("-b:v".into());
        args.push("20M".into());
    }
    args.push("-c:a".into());
    if probe.audio_is_aac {
        args.push("copy".into());
    } else {
        args.push("aac".into());
    }
    args.push("-movflags".into());
    args.push("frag_keyframe+empty_moov+default_base_moof".into());
    args.push("-f".into());
    args.push("mp4".into());
    args.push("pipe:1".into());
    args
}

/// 一个转码会话：持有 ffmpeg 子进程，其 stdout 是 fMP4 流。
pub struct TranscodeSession {
    child: Child,
    pub path: String,
    pub duration_secs: f64,
}

impl TranscodeSession {
    /// 起一个转码会话。start_secs 为起始位置（seek 用）。
    pub fn start(path: &str, start_secs: f64) -> AppResult<Self> {
        let probe = probe(path)?;
        let args = build_args(path, &probe, start_secs);
        let child = Command::new("ffmpeg")
            .args(&args)
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .stdin(Stdio::null())
            .spawn()
            .map_err(|e| AppError::Other(format!("ffmpeg spawn: {e}")))?;
        Ok(TranscodeSession {
            child,
            path: path.to_string(),
            duration_secs: probe.duration_secs,
        })
    }

    /// 从 ffmpeg stdout 读一块数据。返回读到的字节数（0 = 流结束）。
    pub fn read_chunk(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        match self.child.stdout.as_mut() {
            Some(out) => out.read(buf),
            None => Ok(0),
        }
    }

    /// 停止会话：kill ffmpeg 子进程（退出/seek 时调用，瞬时，不阻塞）。
    pub fn stop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

impl Drop for TranscodeSession {
    fn drop(&mut self) {
        self.stop();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn build_args_h264_uses_copy() {
        let p = Probe { video_is_h264: true, audio_is_aac: true, duration_secs: 100.0 };
        let args = build_args("/x.mkv", &p, 0.0);
        let joined = args.join(" ");
        assert!(joined.contains("-c:v copy"));
        assert!(joined.contains("-c:a copy"));
        assert!(joined.contains("pipe:1"));
    }
    #[test]
    fn build_args_hevc_uses_videotoolbox_and_seek() {
        let p = Probe { video_is_h264: false, audio_is_aac: false, duration_secs: 100.0 };
        let args = build_args("/x.mkv", &p, 42.0);
        let joined = args.join(" ");
        assert!(joined.contains("h264_videotoolbox"));
        assert!(joined.contains("aac"));
        assert!(joined.contains("-ss 42"));
    }
}
