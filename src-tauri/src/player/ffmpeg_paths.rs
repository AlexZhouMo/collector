//! 解析并缓存内置 ffmpeg/ffprobe(sidecar) 的绝对路径。
//! Tauri 打包后 sidecar 会剥离 triple 后缀，位于可执行文件旁，
//! 名为 `ffmpeg`/`ffprobe`(Win 带 .exe)。
//! 开发态/测试无 sidecar 时保持未初始化，transcode 回退裸命令名走 PATH。
use std::path::PathBuf;
use std::sync::OnceLock;

/// 解析后的两个二进制绝对路径。init 成功后填充。
struct FfmpegPaths {
    ffmpeg: PathBuf,
    ffprobe: PathBuf,
}

static PATHS: OnceLock<FfmpegPaths> = OnceLock::new();

/// 返回内置 ffmpeg 的绝对路径（已初始化且文件存在时）。
pub fn ffmpeg() -> Option<PathBuf> {
    PATHS.get().map(|p| p.ffmpeg.clone())
}

/// 返回内置 ffprobe 的绝对路径（已初始化且文件存在时）。
pub fn ffprobe() -> Option<PathBuf> {
    PATHS.get().map(|p| p.ffprobe.clone())
}

/// sidecar 文件名：纯二进制名 `<name>`，Windows 追加 .exe。
/// Tauri 打包已剥离 triple 后缀，运行时不需要 triple。
fn sidecar_name(name: &str) -> String {
    if cfg!(windows) { format!("{name}.exe") } else { name.to_string() }
}

/// 在应用启动时调用：解析可执行文件旁的 sidecar 路径，存在则缓存。
pub fn init() {
    let Ok(exe) = std::env::current_exe() else { return };
    let Some(dir) = exe.parent() else { return };
    let ffmpeg = dir.join(sidecar_name("ffmpeg"));
    let ffprobe = dir.join(sidecar_name("ffprobe"));
    if ffmpeg.exists() && ffprobe.exists() {
        let _ = PATHS.set(FfmpegPaths { ffmpeg, ffprobe });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn unset_returns_none() {
        // 测试进程未 init（或 sidecar 不存在），两个 getter 均为 None
        assert!(ffmpeg().is_none());
        assert!(ffprobe().is_none());
    }
    #[test]
    fn sidecar_name_is_bare_binary() {
        let n = sidecar_name("ffmpeg");
        #[cfg(windows)]
        assert_eq!(n, "ffmpeg.exe");
        #[cfg(not(windows))]
        assert_eq!(n, "ffmpeg");
    }
}
