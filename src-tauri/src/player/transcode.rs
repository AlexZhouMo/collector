//! 视频 remux：把 H.264 MKV 用 ffmpeg `-c:v copy` 无损转封装成 fragmented MP4
//! （放缓存目录），前端经本地 HTTP server 用 <video> 边转边播。视频不重编码，
//! 音频转 AAC（WebView <video> 不支持 AC-3/DTS）。缓存复用 + LRU 上限清理。
//!
//! 关键：用 `empty_moov`（moov 在文件头，头部即可初始化 demuxer）替代 `+faststart`
//! （需整片写完二次遍历搬 moov 到头）——后者会强制 ffmpeg 处理到片尾才返回，是"卡死"主因。
//! 转码输出到 `.part` 临时名，后台线程等 ffmpeg 成功后原子 rename 为最终 `.mp4`；
//! 只有最终名代表"完整"，避免半成品被误判复用。
use crate::error::{AppError, AppResult};
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};

/// 缓存上限（字节）。默认 20 GiB。
pub const CACHE_LIMIT_BYTES: u64 = 20 * 1024 * 1024 * 1024;

/// 定位 ffmpeg/ffprobe：优先用启动时解析的内置 sidecar 绝对路径；
/// 开发态/未初始化时回退裸命令名走 PATH（便于 tauri dev / cargo test）。
fn ffmpeg_bin(name: &str) -> PathBuf {
    let resolved = match name {
        "ffmpeg" => crate::player::ffmpeg_paths::ffmpeg(),
        "ffprobe" => crate::player::ffmpeg_paths::ffprobe(),
        _ => None,
    };
    resolved.unwrap_or_else(|| PathBuf::from(name))
}

/// 检测 ffmpeg 与 ffprobe 是否可用（能 spawn 且 -version 成功）。
/// 缺失时返回含详细引导的 Err，供前端展示。
fn ensure_ffmpeg_available() -> AppResult<()> {
    for tool in ["ffmpeg", "ffprobe"] {
        let ok = Command::new(ffmpeg_bin(tool))
            .arg("-version")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false);
        if !ok {
            return Err(AppError::Other(ffmpeg_missing_hint(tool)));
        }
    }
    Ok(())
}

/// 内置 ffmpeg/ffprobe 缺失或损坏时的中文提示（引导重装）。
fn ffmpeg_missing_hint(tool: &str) -> String {
    format!(
        "视频播放需要内置的 {tool} 组件，但未找到或无法运行。\n\n\
         Collector 已随安装包内置 ffmpeg/ffprobe，此提示通常意味着安装文件损坏或被安全软件拦截。\n\n\
         【解决办法】请重新下载并安装 Collector；若仍失败，可将应用加入安全软件白名单后重试。"
    )
}

/// 用 ffprobe 取视频时长（秒）。失败返回 Err。
pub fn probe_duration(path: &str) -> AppResult<f64> {
    let out = Command::new(ffmpeg_bin("ffprobe"))
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

/// 缓存目录下某源视频对应的 mp4 产物路径（路径 hash 命名，稳定可复用）。
pub fn cached_mp4_path(cache_dir: &Path, src: &str) -> PathBuf {
    let mut h = DefaultHasher::new();
    src.hash(&mut h);
    cache_dir.join(format!("collector_{:016x}.mp4", h.finish()))
}

/// 转码中间产物路径：最终 mp4 加 `.part` 后缀。只有 rename 成最终名才算"完整"。
pub fn part_path(final_mp4: &Path) -> PathBuf {
    let mut s = final_mp4.as_os_str().to_os_string();
    s.push(".part");
    PathBuf::from(s)
}

/// 更新 mtime 为现在（复用时调用，让常看的视频在 LRU 中更新鲜）。
pub fn touch(p: &Path) {
    let _ = filetime::set_file_mtime(p, filetime::FileTime::now());
}

/// 强制缓存目录总大小 <= max_bytes：超出按 mtime 从最旧删起。只处理 collector_*.mp4。
pub fn enforce_cache_limit(cache_dir: &Path, max_bytes: u64) {
    let mut files: Vec<(PathBuf, u64, filetime::FileTime)> = Vec::new();
    let rd = match std::fs::read_dir(cache_dir) { Ok(r) => r, Err(_) => return };
    for e in rd.filter_map(|e| e.ok()) {
        let p = e.path();
        let name = p.file_name().and_then(|s| s.to_str()).unwrap_or("");
        if !(name.starts_with("collector_") && name.ends_with(".mp4")) { continue; }
        if let Ok(m) = e.metadata() {
            files.push((p.clone(), m.len(), filetime::FileTime::from_last_modification_time(&m)));
        }
    }
    let total: u64 = files.iter().map(|(_, s, _)| *s).sum();
    if total <= max_bytes { return; }
    files.sort_by_key(|(_, _, t)| *t); // 最旧在前
    let mut cur = total;
    for (p, size, _) in files {
        if cur <= max_bytes { break; }
        if std::fs::remove_file(&p).is_ok() { cur -= size; }
    }
}

/// `start_remux` 的结果：产物已完整（秒开复用）或已后台启动转码。
pub enum TranscodeStatus {
    /// 完整产物已存在，直接播放。附最终路径与时长。
    Ready(PathBuf, f64),
    /// 已后台 spawn ffmpeg，边转边播。附子进程 handle 与时长。
    Started(TranscodeHandle, f64),
}

/// 后台转码 handle：子进程 + 两个路径。调用方负责保存 child（供 kill）并读 stderr 进度。
pub struct TranscodeHandle {
    pub child: Child,
    pub part_path: PathBuf,
    pub final_path: PathBuf,
}

/// ffmpeg 转普通 MP4 的参数（不含输入/输出/进度重定向）。
/// `+faststart`：转码完成后把 moov 原子移到文件头，WKWebView/AVFoundation 能识别轨道播放。
/// （不用 fragmented MP4：AVFoundation 不支持渐进解析 fmp4，增长中的文件被判 isPlayable=false，
/// 故本方案转码完成后才播——完整产物 moov 在头，秒开可 seek。）
/// 视频 `-c:v copy` 无损直拷（快），音频转 AAC（WebView 不支持 AC-3/DTS）。
fn fmp4_args(input: &str, out_part: &str) -> Vec<String> {
    vec![
        "-nostdin".into(),
        "-i".into(), input.into(),
        "-c:v".into(), "copy".into(),
        "-c:a".into(), "aac".into(),
        "-b:a".into(), "192k".into(),
        "-movflags".into(), "+faststart".into(),
        "-f".into(), "mp4".into(),
        "-progress".into(), "pipe:2".into(),
        "-y".into(),
        out_part.into(),
    ]
}

/// 打开视频：若完整产物已存在则 `Ready`（秒开复用，兼容旧 faststart 产物）；
/// 否则删旧 `.part`、后台 spawn ffmpeg 转 fragmented MP4 到 `.part`，返回 `Started`。
/// 不阻塞等转码完成——调用方保存 handle、读 stderr 进度、成功后 rename `.part`→最终名。
/// 视频无损保留（H.264 直进 MP4）；音频转 AAC（WebView <video> 不支持 AC-3/DTS）。
pub fn start_remux(cache_dir: &Path, path: &str) -> AppResult<TranscodeStatus> {
    ensure_ffmpeg_available()?;
    std::fs::create_dir_all(cache_dir).ok();
    let duration = probe_duration(path)?;
    let out = cached_mp4_path(cache_dir, path);
    // 复用：最终名存在且非空则完整，直接用（向后兼容旧 faststart mp4）
    if out.metadata().map(|m| m.len() > 0).unwrap_or(false) {
        touch(&out); // 复用：更新 mtime 供 LRU 识别"最近用过"
        return Ok(TranscodeStatus::Ready(out, duration));
    }
    // 清理可能残留的旧 .part（上次转码中途退出/崩溃留下的半成品）
    let part = part_path(&out);
    let _ = std::fs::remove_file(&part);
    let child = Command::new(ffmpeg_bin("ffmpeg"))
        .args(fmp4_args(path, &part.to_string_lossy()))
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| AppError::Other(format!("ffmpeg spawn: {e}")))?;
    Ok(TranscodeStatus::Started(
        TranscodeHandle { child, part_path: part, final_path: out },
        duration,
    ))
}

/// 转码成功后调用：把 `.part` 原子 rename 为最终 `.mp4`，再做 LRU 清理。
/// rename 失败（如 Windows 文件被占用）时保留 `.part`，下次重转。
pub fn finalize_remux(part: &Path, final_mp4: &Path, cache_dir: &Path) -> AppResult<()> {
    std::fs::rename(part, final_mp4)
        .map_err(|e| AppError::Other(format!("finalize rename: {e}")))?;
    enforce_cache_limit(cache_dir, CACHE_LIMIT_BYTES);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    #[test]
    fn cached_path_is_stable_and_mp4() {
        let dir = Path::new("/cache");
        let a = cached_mp4_path(dir, "/movies/x.mkv");
        assert_eq!(a, cached_mp4_path(dir, "/movies/x.mkv"));
        assert!(a.starts_with("/cache"));
        assert!(a.to_string_lossy().ends_with(".mp4"));
        assert_ne!(a, cached_mp4_path(dir, "/movies/y.mkv"));
    }
    #[test]
    fn part_path_appends_suffix() {
        let final_mp4 = Path::new("/cache/collector_abc.mp4");
        assert_eq!(part_path(final_mp4), PathBuf::from("/cache/collector_abc.mp4.part"));
    }
    #[test]
    fn transcode_args_use_faststart_not_fragmented() {
        let args = fmp4_args("/in.mkv", "/out.mp4.part");
        let joined = args.join(" ");
        assert!(joined.contains("faststart"));
        assert!(!joined.contains("empty_moov"));
        assert!(!joined.contains("frag_keyframe"));
        assert!(joined.contains("-c:v copy"));
        assert!(joined.contains("-progress pipe:2"));
        assert!(joined.ends_with("/out.mp4.part"));
    }
    #[test]
    fn lru_deletes_oldest_over_limit() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path();
        for (i, name) in ["collector_a.mp4", "collector_b.mp4", "collector_c.mp4"].iter().enumerate() {
            let p = dir.join(name);
            std::fs::File::create(&p).unwrap().write_all(&vec![0u8; 1000]).unwrap();
            filetime::set_file_mtime(&p, filetime::FileTime::from_unix_time(1000 + i as i64, 0)).unwrap();
        }
        enforce_cache_limit(dir, 2500); // 3000→需删到<=2500，删最旧 a
        assert!(!dir.join("collector_a.mp4").exists());
        assert!(dir.join("collector_b.mp4").exists());
        assert!(dir.join("collector_c.mp4").exists());
    }
    #[test]
    fn ffmpeg_hint_mentions_reinstall() {
        let h = ffmpeg_missing_hint("ffmpeg");
        assert!(h.contains("内置"));
        assert!(h.contains("重新下载并安装 Collector"));
        assert!(h.contains("ffmpeg"));
    }
}
