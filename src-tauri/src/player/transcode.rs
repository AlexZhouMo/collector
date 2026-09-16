//! 视频 remux：把 H.264 MKV 用 ffmpeg `-c copy` 无损转封装成 MP4（放缓存目录，
//! asset:// 可访问），再由前端经 asset:// 协议用 <video> 播放。不重编码（秒级），
//! 不做实时转码流。缓存复用 + LRU 上限清理。
use crate::error::{AppError, AppResult};
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::process::Command;

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

/// remux 到缓存目录（asset 可访问的应用数据目录）。同源已 remux 过则复用。
/// 视频无损保留（H.264 直接进 MP4）；音频转 AAC——WebView <video> 不支持
/// AC-3/DTS 等，遇到非 AAC 音频整个媒体会解码失败（画面也黑）。转 AAC 很轻。
/// 非 H.264 等 `-c copy` 不兼容 MP4 的编码会导致 ffmpeg 失败，返回 Err（前端提示不支持）。
pub fn remux(cache_dir: &Path, path: &str) -> AppResult<(String, f64)> {
    ensure_ffmpeg_available()?;
    std::fs::create_dir_all(cache_dir).ok();
    let duration = probe_duration(path)?;
    let out = cached_mp4_path(cache_dir, path);
    // 复用：产物已存在且非空则直接用
    if out.metadata().map(|m| m.len() > 0).unwrap_or(false) {
        touch(&out); // 复用：更新 mtime 供 LRU 识别"最近用过"
        return Ok((out.to_string_lossy().into_owned(), duration));
    }
    let status = Command::new(ffmpeg_bin("ffmpeg"))
        .args([
            "-nostdin",
            "-i", path,
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
    enforce_cache_limit(cache_dir, CACHE_LIMIT_BYTES); // 产出后清理超限
    Ok((out.to_string_lossy().into_owned(), duration))
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
