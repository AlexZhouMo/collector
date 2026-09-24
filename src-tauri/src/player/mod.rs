//! 视频播放：H.264 MKV 用 ffmpeg 转成 HLS（`.m3u8` + `.ts` segments），
//! 前端 <video> + hls.js 走 MSE 播放（Chromium 无原生 HLS）。
//! - AC-3 音轨在 Windows 上被静音——转 AAC；其他音频保持 copy。
//! - HLS 段边转边写，首段（~2s）就绪即可起播；缓存复用，磁盘由 LRU 管理。
//! - 文件从 webview 到磁盘的通道：见 [lib.rs](../lib.rs.html) 的 `register_uri_scheme_protocol("hls", ...)`
//!   自定义 URI 协议 handler（Tauri asset 协议对 fetch 返 404，本地 HTTP server 被
//!   WebView2 拦截，所以自建 URI 协议）。
pub mod transcode;
pub mod ffmpeg_paths;

use crate::error::AppResult;
use serde::Serialize;
use std::path::PathBuf;
use std::process::Child;
use std::sync::Mutex;
use tauri::Manager;

/// 当前播放会话：转码组前缀 + 后台 ffmpeg 子进程 + stdin 句柄。同一时刻只播一个视频。
/// 完整产物播放（`Ready`）时 child/stdin/prefix 均为 None。
#[derive(Default)]
pub struct PlaySession {
    /// 后台 ffmpeg 子进程（仅转码中分支有）。停止/切换时通过 stdin 写 `q\n` 优雅退出。
    pub child: Option<Child>,
    /// ffmpeg stdin 句柄——写 `q\n` 让 ffmpeg 写完当前段 + 追加 ENDLIST 后退出。
    pub stdin: Option<std::process::ChildStdin>,
    /// 缓存组前缀 `collector_HASH`（仅转码中分支有）。切换视频 kill 时用来清 .tmp 残留。
    pub cache_prefix: Option<String>,
}

/// 当前播放会话（同一时刻只播一个视频）。
#[derive(Default)]
pub struct PlayerState(pub Mutex<Option<PlaySession>>);

/// player_open 返回给前端：HLS playlist 的文件名（前端拼 `http://hls.localhost/文件名`）+ 时长。
#[derive(Serialize)]
pub struct PlayerInfo {
    pub src: String,
    pub duration: f64,
    pub subtitle: Option<String>,
}

/// 按 category/category_path/title 推导字幕明文路径，存在则返回绝对路径。
fn resolve_subtitle(app_data: &str, category: &str, category_path: &str, title: &str) -> Option<String> {
    let abs = crate::library::paths::subtitle_abs_path(app_data, category, category_path, title);
    if std::path::Path::new(&abs).is_file() { Some(abs) } else { None }
}

/// 让 ffmpeg 优雅退出：往 stdin 写 `q\n`——ffmpeg 会写完当前段（rename `.ts.tmp`→`.ts`），
/// 更新 playlist（追加 EXTINF），关闭输出。最后写 ENDLIST 只发生在 EOF 到达源末尾时；
/// 用户中途退出 → 没有 ENDLIST，下次 `start_remux` 走续转分支。
/// 不到 3 秒仍未退出则强 kill（防止 ffmpeg 卡死拖住 UI）。返回时子进程已收尾。
fn graceful_stop(mut child: Child, stdin: Option<std::process::ChildStdin>) {
    use std::io::Write;
    if let Some(mut s) = stdin {
        // 忽略写错误（ffmpeg 可能已经因源结束等原因退出了）
        let _ = s.write_all(b"q\n");
        let _ = s.flush();
        drop(s); // 关 stdin，让 ffmpeg 也走 EOF 路径以防它没监听 stdin
    }
    // 短窗口等自然退出；超时就强杀
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
    loop {
        match child.try_wait() {
            Ok(Some(_)) => return, // 已退出
            Ok(None) => {
                if std::time::Instant::now() >= deadline {
                    let _ = child.kill();
                    let _ = child.wait();
                    return;
                }
                std::thread::sleep(std::time::Duration::from_millis(50));
            }
            Err(_) => {
                let _ = child.kill();
                let _ = child.wait();
                return;
            }
        }
    }
}

/// 切换视频/清缓存时用：优雅停 ffmpeg，然后按需清残留。
/// - `preserve_partial=true`（player_open 换视频）：只清 `.ts.tmp` 半吊子段，保留完整 `.m3u8` + `.ts`
///   供以后可能的续转/复用
/// - `preserve_partial=false`（cache_clear）：整组删（`.m3u8` + `.ts` + `.tmp`）
fn stop_session(
    child: Option<Child>,
    stdin: Option<std::process::ChildStdin>,
    cache_prefix: Option<String>,
    cache_dir: &std::path::Path,
    preserve_partial: bool,
) {
    if let Some(c) = child {
        graceful_stop(c, stdin);
    }
    let prefix = match cache_prefix { Some(p) => p, None => return };
    // 无论如何都清 .ts.tmp（半段写一半的，续转时会作废）
    let rd = match std::fs::read_dir(cache_dir) { Ok(r) => r, Err(_) => return };
    if !preserve_partial {
        // 整组删（含 playlist、完整 .ts、半吊 .tmp）
        for e in rd.filter_map(|e| e.ok()) {
            let name = match e.file_name().into_string() { Ok(n) => n, Err(_) => continue };
            if name.starts_with(&prefix)
                && (name.ends_with(".m3u8") || name.ends_with(".ts") || name.ends_with(".ts.tmp"))
            {
                let _ = std::fs::remove_file(e.path());
            }
        }
    } else {
        // 只清 .ts.tmp（半吊段）；.m3u8 + 已完成 .ts 保留供下次续转
        for e in rd.filter_map(|e| e.ok()) {
            let name = match e.file_name().into_string() { Ok(n) => n, Err(_) => continue };
            if name.starts_with(&prefix) && name.ends_with(".ts.tmp") {
                let _ = std::fs::remove_file(e.path());
            }
        }
    }
}

/// 打开视频：完整 HLS 缓存复用则直接返回 playlist 文件名；否则后台 spawn ffmpeg
/// 增量写 HLS segments，立即返回 playlist 文件名——hls.js 会周期性 refetch 直到
/// 看到 `#EXT-X-ENDLIST`。前端拼 `http://hls.localhost/<文件名>` 走 lib.rs 里的
/// 自定义 URI 协议 handler。
#[tauri::command(rename_all = "camelCase")]
pub fn player_open(
    app: tauri::AppHandle,
    db: tauri::State<crate::db::Db>,
    state: tauri::State<PlayerState>,
    category: String,
    category_path: String,
    title: String,
) -> AppResult<PlayerInfo> {
    let root = crate::settings::get(&db, crate::library::paths::video_root_key(&category))?
        .unwrap_or_default();
    let abs = crate::library::paths::video_abs_path(&root, &category_path, &title);
    if abs.is_empty() || !std::path::Path::new(&abs).is_file() {
        return Err(crate::error::AppError::Other("视频文件不存在".into()));
    }
    let app_data = app
        .path()
        .app_data_dir()
        .map_err(|e| crate::error::AppError::Other(format!("app_data_dir: {e}")))?;
    let cache_dir = app_data.join("video_cache");

    // 先停旧会话：优雅退出（保留部分缓存供续转/复用）
    let prev = state.0.lock().unwrap().take();
    if let Some(mut s) = prev {
        stop_session(
            s.child.take(),
            s.stdin.take(),
            s.cache_prefix.take(),
            &cache_dir,
            true, // preserve partial
        );
    }

    let status = transcode::start_remux(&cache_dir, &abs)?;

    let (playlist_path, duration, session) = match status {
        transcode::TranscodeStatus::Ready(path, duration) => {
            let final_path = path.to_string_lossy().into_owned();
            let session = PlaySession::default();
            (final_path, duration, session)
        }
        transcode::TranscodeStatus::Started(mut handle, duration) => {
            let final_path = handle.final_path.to_string_lossy().into_owned();
            // 起后台线程：drain ffmpeg stderr，进程结束时 LRU + 失败清理。
            let stderr = handle.child.stderr.take();
            let final_p = handle.final_path.clone();
            let cache = cache_dir.clone();
            let prefix_for_worker = handle.cache_prefix.clone();
            if let Some(stderr) = stderr {
                std::thread::spawn(move || {
                    finalize_worker(stderr, final_p, cache, prefix_for_worker);
                });
            }
            let session = PlaySession {
                child: Some(handle.child),
                stdin: handle.stdin,
                cache_prefix: Some(handle.cache_prefix),
            };
            (final_path, duration, session)
        }
    };

    let file_name = std::path::Path::new(&playlist_path)
        .file_name()
        .and_then(|s| s.to_str())
        .ok_or_else(|| crate::error::AppError::Other("bad cache file name".into()))?;
    // 返回文件名——前端拼成 `http://hls.localhost/<file_name>`（自定义 URI 协议，
    // 见 lib.rs `register_uri_scheme_protocol("hls", ...)`）。
    // 不用 asset 协议：那个对 `<img>` 加载 OK，对 fetch/XHR 返 404（WebView2 里 media
    // 加载与 fetch 走不同路径）。hls.js 靠 fetch 加载 manifest/段，必须自定义协议。
    let src = file_name.to_string();
    let app_data_str = app_data.to_string_lossy().to_string();
    let subtitle = resolve_subtitle(&app_data_str, &category, &category_path, &title);
    *state.0.lock().unwrap() = Some(session);
    Ok(PlayerInfo { src, duration, subtitle })
}

/// 后台线程：drain ffmpeg stderr（含 `-progress pipe:2` 输出），进程结束后按退出
/// 标志判定成功。成功则 LRU 清理；**失败/优雅退出保留已转部分**（供下次续转）——
/// 只有用户手动 cache_clear 才会整组删除。
///
/// 不再向前端 emit 进度——hls.js 自己会通过 canplay/waiting/error 事件反馈状态；
/// 用户 seek 到未转码位置就是 hls.js 拉不到段 → 短暂 buffering，正常 VOD 体验。
fn finalize_worker(
    stderr: std::process::ChildStderr,
    final_path: PathBuf,
    cache_dir: PathBuf,
    cache_prefix: String,
) {
    use std::io::{BufRead, BufReader};
    let _ = cache_prefix; // 现在保留部分缓存，prefix 不用于删除
    let reader = BufReader::new(stderr);
    let mut saw_end = false;
    for line in reader.lines().map_while(Result::ok) {
        if line.strip_prefix("progress=") == Some("end") {
            saw_end = true;
        }
    }
    // 成功完成（源到 EOF、playlist 追加了 ENDLIST）→ LRU 清理。否则（q\n 优雅退出 / 崩溃）
    // 保留 .m3u8 + .ts 段供下次 `start_remux` 走续转分支。
    if saw_end && transcode::playlist_is_complete(&final_path) {
        transcode::finalize_remux(&cache_dir);
    }
}

/// 停止播放：**优雅退出** ffmpeg（写 `q\n` 让其收尾当前段），**保留已转部分**——
/// 下次打开同视频走 `start_remux` 的续转分支，从上次退出的段号继续追加。
/// 转完部分位于磁盘上，设置页缓存利用率会立即反映。
#[tauri::command]
pub fn player_stop(app: tauri::AppHandle, state: tauri::State<PlayerState>) -> AppResult<()> {
    let cache_dir = video_cache_dir(&app)?;
    let prev = state.0.lock().unwrap().take();
    if let Some(mut s) = prev {
        stop_session(
            s.child.take(),
            s.stdin.take(),
            s.cache_prefix.take(),
            &cache_dir,
            true, // preserve partial for future resume
        );
    }
    Ok(())
}

/// 视频缓存目录路径 + 已用字节数 + 上限字节数（供设置页显示）。
#[derive(Serialize)]
pub struct CacheInfo {
    pub path: String,
    pub used_bytes: u64,
    pub limit_bytes: u64,
}

/// 取视频缓存目录（<app_data>/video_cache）。
fn video_cache_dir(app: &tauri::AppHandle) -> AppResult<PathBuf> {
    let app_data = app
        .path()
        .app_data_dir()
        .map_err(|e| crate::error::AppError::Other(format!("app_data_dir: {e}")))?;
    Ok(app_data.join("video_cache"))
}

/// 查询视频缓存信息：目录路径 + 已用字节数 + 上限字节数。
#[tauri::command]
pub fn cache_info(app: tauri::AppHandle) -> AppResult<CacheInfo> {
    let cache_dir = video_cache_dir(&app)?;
    let used_bytes = transcode::cache_used_bytes(&cache_dir);
    Ok(CacheInfo {
        path: cache_dir.to_string_lossy().into_owned(),
        used_bytes,
        limit_bytes: transcode::CACHE_LIMIT_BYTES,
    })
}

/// 清空视频缓存：先停当前会话（优雅退出 + 整组删），再删所有本模块管理的文件。
/// 返回删除的文件数。
#[tauri::command]
pub fn cache_clear(app: tauri::AppHandle, state: tauri::State<PlayerState>) -> AppResult<usize> {
    let cache_dir = video_cache_dir(&app)?;
    let prev = state.0.lock().unwrap().take();
    if let Some(mut s) = prev {
        stop_session(
            s.child.take(),
            s.stdin.take(),
            s.cache_prefix.take(),
            &cache_dir,
            false, // 清缓存：整组删
        );
    }
    Ok(transcode::clear_cache(&cache_dir))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_subtitle_by_path() {
        let tmp = tempfile::tempdir().unwrap();
        let app = tmp.path().to_string_lossy().to_string();
        let dir = tmp.path().join("subtitles/电影/科幻");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("星战.ass"), "x").unwrap();
        assert!(resolve_subtitle(&app, "电影", "科幻", "星战").is_some());
        assert!(resolve_subtitle(&app, "电影", "科幻", "不存在").is_none());
    }
}
