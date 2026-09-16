//! 本地视频 HTTP 服务：为 `<video>` 提供支持 Range/206 的流式播放。
//!
//! 为什么需要：Tauri asset:// 及自定义 wry 协议的响应 body 是一次性缓冲，
//! 非 Range 请求会把整个文件读进内存——GB 级视频必然失败（MediaError code 4）。
//! 真正的 HTTP server 原生支持 Range，`<video>` 可按需拉取分段、seek 走 206，
//! 不把整文件读进内存。只服务视频缓存目录下的 collector_*.mp4，仅监听 127.0.0.1。
//!
//! 边转边播：请求的最终 `.mp4` 不存在时回退读同名 `.part`（正在增长的中间产物）。
//! 增长文件总长未知，用 `Content-Range: .../*`；seek 到尚未写入的偏移则短暂轮询等待。
//! 每请求 spawn 线程处理，避免一个"等待未写偏移"的挂起请求阻塞后续所有请求。
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tiny_http::{Header, Response, Server, StatusCode};

/// seek 到尚未写入偏移时最多等待多久（ffmpeg 追上该处）。超时则 416。
const GROW_WAIT_TIMEOUT: Duration = Duration::from_secs(10);
/// 轮询增长文件大小的间隔。
const GROW_POLL_INTERVAL: Duration = Duration::from_millis(100);
/// 单次 Range 响应的最大字节数。WKWebView 请求 `bytes=0-<整个文件>` 时，若一次串流
/// 整个 GB 级响应会拖住其媒体管线（迟迟不 canplay），故按此上限切块，让客户端增量拉取。
const MAX_RANGE_CHUNK: u64 = 8 * 1024 * 1024;

/// 从 URL 路径取安全的缓存文件绝对路径。只接受 `/collector_<...>.mp4` 形式的
/// 单层文件名，拒绝路径穿越（`..`、`/`、绝对路径）。非法返回 None。
pub fn safe_cache_file(cache_dir: &Path, url_path: &str) -> Option<PathBuf> {
    let name = url_path.trim_start_matches('/');
    // 只允许 collector_*.mp4 单层文件名，不含任何路径分隔或 ..
    if name.contains('/') || name.contains("..") || name.contains('\\') {
        return None;
    }
    if !(name.starts_with("collector_") && name.ends_with(".mp4")) {
        return None;
    }
    Some(cache_dir.join(name))
}

/// 解析 `Range: bytes=start-end` 头。返回 (start, end_inclusive_option)。
/// 仅支持 `bytes=start-`、`bytes=start-end`。不支持多段/后缀范围（返回 None）。
pub fn parse_range(header: &str, file_len: u64) -> Option<(u64, u64)> {
    let spec = header.strip_prefix("bytes=")?;
    let (s, e) = spec.split_once('-')?;
    let start: u64 = s.trim().parse().ok()?;
    if start >= file_len {
        return None;
    }
    let end: u64 = if e.trim().is_empty() {
        file_len - 1
    } else {
        e.trim().parse::<u64>().ok()?.min(file_len - 1)
    };
    if end < start {
        return None;
    }
    Some((start, end))
}

fn header(name: &str, value: &str) -> Header {
    Header::from_bytes(name.as_bytes(), value.as_bytes()).unwrap()
}

/// 在 127.0.0.1 随机端口启动 HTTP server（后台线程），只服务 cache_dir 下的
/// collector_*.mp4（及转码中的同名 .part）。每请求 spawn 线程处理，避免一个
/// "等待未写偏移"的挂起请求阻塞后续所有请求。返回分配到的端口。
pub fn start(cache_dir: PathBuf) -> std::io::Result<u16> {
    let server = Server::http("127.0.0.1:0").map_err(|e| {
        std::io::Error::new(std::io::ErrorKind::Other, format!("http server: {e}"))
    })?;
    let port = server.server_addr().to_ip().map(|a| a.port()).unwrap_or(0);
    let server = Arc::new(server);
    std::thread::spawn(move || {
        let cache_dir = Arc::new(cache_dir);
        for request in server.incoming_requests() {
            let cache_dir = Arc::clone(&cache_dir);
            std::thread::spawn(move || handle(request, &cache_dir));
        }
    });
    Ok(port)
}

/// 解析请求想要的文件：优先最终 `.mp4`；不存在则回退同名 `.part`（转码中）。
/// 返回 (路径, 是否为增长中的 .part)。都不存在返回 None。
fn resolve_target(cache_dir: &Path, url: &str) -> Option<(PathBuf, bool)> {
    let final_mp4 = safe_cache_file(cache_dir, url)?;
    if final_mp4.is_file() {
        return Some((final_mp4, false));
    }
    let mut part = final_mp4.into_os_string();
    part.push(".part");
    let part = PathBuf::from(part);
    if part.is_file() {
        return Some((part, true));
    }
    None
}

fn handle(request: tiny_http::Request, cache_dir: &Path) {
    let url = request.url().to_string();
    let (path, growing) = match resolve_target(cache_dir, &url) {
        Some(t) => t,
        None => {
            let _ = request.respond(Response::empty(StatusCode(404)));
            return;
        }
    };
    let mut file = match File::open(&path) {
        Ok(f) => f,
        Err(_) => {
            let _ = request.respond(Response::empty(StatusCode(404)));
            return;
        }
    };
    let range_hdr = request
        .headers()
        .iter()
        .find(|h| h.field.equiv("Range"))
        .map(|h| h.value.as_str().to_string());

    if growing {
        serve_growing(request, &path, &mut file, range_hdr.as_deref());
        return;
    }
    // 完整文件：已知总长，走原逻辑（精确 Content-Range/Content-Length）。
    let total = match file.metadata() {
        Ok(m) => m.len(),
        Err(_) => {
            let _ = request.respond(Response::empty(StatusCode(500)));
            return;
        }
    };
    if let Some((start, mut end)) = range_hdr.as_deref().and_then(|r| parse_range(r, total)) {
        // 单次响应上限：WKWebView 有时请求 `bytes=0-<整个文件>`，若忠实串流整个 GB 级
        // 响应，其媒体管线会等待大量数据到达才 canplay，表现为"打开卡顿"。限制单块大小，
        // 让客户端按需用后续 Range 增量拉取（Chromium 也是这种小块增量模式）。
        let cap_end = start + MAX_RANGE_CHUNK - 1;
        if end > cap_end {
            end = cap_end.min(total - 1);
        }
        // 206 Partial Content：seek 到 start，只读 [start,end]，流式返回（不读全文件）
        let len = end - start + 1;
        if file.seek(SeekFrom::Start(start)).is_err() {
            let _ = request.respond(Response::empty(StatusCode(500)));
            return;
        }
        let reader = file.take(len);
        let resp = Response::empty(StatusCode(206))
            .with_data(reader, Some(len as usize))
            .with_header(header("Content-Type", "video/mp4"))
            .with_header(header("Accept-Ranges", "bytes"))
            .with_header(header(
                "Content-Range",
                &format!("bytes {start}-{end}/{total}"),
            ));
        let _ = request.respond(resp);
    } else {
        // 无 Range：200 但只回开头一块（同上，避免一次性塞整个文件）。
        // 客户端拿到 Accept-Ranges 后会改用 Range 增量拉取。
        let end = (MAX_RANGE_CHUNK - 1).min(total - 1);
        let len = end + 1;
        let reader = file.take(len);
        let resp = Response::empty(StatusCode(206))
            .with_data(reader, Some(len as usize))
            .with_header(header("Content-Type", "video/mp4"))
            .with_header(header("Accept-Ranges", "bytes"))
            .with_header(header("Content-Range", &format!("bytes 0-{end}/{total}")));
        let _ = request.respond(resp);
    }
}

/// 服务正在增长的 `.part` 文件：总长未知，用 `Content-Range: .../*`。
/// seek 到尚未写入的偏移则短暂轮询等待 ffmpeg 追上；超时返回 416。
fn serve_growing(
    request: tiny_http::Request,
    path: &Path,
    file: &mut File,
    range_hdr: Option<&str>,
) {
    // 解析 Range 起点；无 Range 视为从 0 开始的开放式请求。
    let start: u64 = match range_hdr {
        Some(r) => match r.strip_prefix("bytes=").and_then(|s| s.split_once('-')) {
            Some((s, _)) => match s.trim().parse() {
                Ok(v) => v,
                Err(_) => {
                    let _ = request.respond(Response::empty(StatusCode(416)));
                    return;
                }
            },
            None => 0,
        },
        None => 0,
    };
    // 等待文件增长到 start 之后（ffmpeg 写到该处）。超时 416。
    let size = match wait_for_offset(path, start) {
        Some(s) => s,
        None => {
            let _ = request.respond(Response::empty(StatusCode(416)));
            return;
        }
    };
    let end = (size - 1).min(start + MAX_RANGE_CHUNK - 1); // 只回当前已写入末尾，且不超过单块上限
    let len = end - start + 1;
    if file.seek(SeekFrom::Start(start)).is_err() {
        let _ = request.respond(Response::empty(StatusCode(500)));
        return;
    }
    let reader = file.take(len);
    // total 未知用 `*`；<video> 据此知道还有更多数据、按需继续拉取。
    let resp = Response::empty(StatusCode(206))
        .with_data(reader, Some(len as usize))
        .with_header(header("Content-Type", "video/mp4"))
        .with_header(header("Accept-Ranges", "bytes"))
        .with_header(header("Content-Range", &format!("bytes {start}-{end}/*")));
    let _ = request.respond(resp);
}

/// 轮询等待文件大小超过 offset（即 offset 处已有数据可读）。
/// 返回等到的当前文件大小；超时或文件消失返回 None。
fn wait_for_offset(path: &Path, offset: u64) -> Option<u64> {
    let deadline = Instant::now() + GROW_WAIT_TIMEOUT;
    loop {
        match std::fs::metadata(path) {
            Ok(m) if m.len() > offset => return Some(m.len()),
            // 文件已被 rename 成最终名（转码完成）：.part 消失，让客户端重试拿完整文件
            Err(_) => return None,
            _ => {}
        }
        if Instant::now() >= deadline {
            return None;
        }
        std::thread::sleep(GROW_POLL_INTERVAL);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_range_open_ended() {
        assert_eq!(parse_range("bytes=100-", 1000), Some((100, 999)));
    }
    #[test]
    fn parse_range_closed() {
        assert_eq!(parse_range("bytes=100-200", 1000), Some((100, 200)));
    }
    #[test]
    fn parse_range_end_clamped_to_file() {
        assert_eq!(parse_range("bytes=100-99999", 1000), Some((100, 999)));
    }
    #[test]
    fn parse_range_rejects_out_of_bounds_start() {
        assert_eq!(parse_range("bytes=2000-", 1000), None);
    }
    #[test]
    fn safe_file_accepts_valid_name() {
        let dir = Path::new("/cache");
        assert_eq!(
            safe_cache_file(dir, "/collector_abc.mp4"),
            Some(PathBuf::from("/cache/collector_abc.mp4"))
        );
    }
    #[test]
    fn safe_file_rejects_traversal_and_others() {
        let dir = Path::new("/cache");
        assert_eq!(safe_cache_file(dir, "/../etc/passwd"), None);
        assert_eq!(safe_cache_file(dir, "/collector_a/../x.mp4"), None);
        assert_eq!(safe_cache_file(dir, "/other.mp4"), None); // 非 collector_ 前缀
        assert_eq!(safe_cache_file(dir, "/collector_a.txt"), None); // 非 .mp4
    }
}
