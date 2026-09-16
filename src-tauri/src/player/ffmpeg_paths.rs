//! 解析并缓存内置 ffmpeg/ffprobe(sidecar) 的绝对路径。
//! 打包后 sidecar 位于可执行文件旁，命名为 `ffmpeg-<target-triple>`(Win 带 .exe)。
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

/// sidecar 文件名：`<name>-<triple>`，Windows 追加 .exe。
/// triple 由构建期 env `TARGET`（tauri-build 注入 `TAURI_ENV_TARGET_TRIPLE`）决定。
fn sidecar_name(name: &str) -> String {
    let triple = option_env!("TAURI_ENV_TARGET_TRIPLE").unwrap_or("");
    let base = if triple.is_empty() { name.to_string() } else { format!("{name}-{triple}") };
    if cfg!(windows) { format!("{base}.exe") } else { base }
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
    fn sidecar_name_has_triple_or_bare() {
        let n = sidecar_name("ffmpeg");
        // 至少包含基名；Windows 带 .exe
        assert!(n.contains("ffmpeg"));
        #[cfg(windows)]
        assert!(n.ends_with(".exe"));
    }
}
