//! 视频播放：H.264 MKV 转封装成 fragmented MP4（放应用数据缓存目录），前端 <video>
//! 经本地 HTTP server（支持 Range/206 流式）边转边播。缓存产物不随停止删除，磁盘由 LRU 管理。
pub mod httpserver;
pub mod transcode;
pub mod ffmpeg_paths;
#[cfg(test)]
mod mpv_probe;

use crate::error::AppResult;
use serde::Serialize;
use std::path::PathBuf;
use std::process::Child;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use tauri::{Emitter, Manager};

/// 播放会话代号，单调递增。每次 player_open 取新值，用于让旧转码的进度事件失效
/// （快速切换视频时，旧会话的 transcode-progress 不应影响新会话）。
static EPOCH: AtomicU64 = AtomicU64::new(0);

/// 当前播放会话：转码中间产物 + 后台 ffmpeg 子进程。同一时刻只播一个视频。
/// 仅在停止/切换时用于 kill 子进程、清残留 `.part`；完整产物播放时二者均为 None。
#[derive(Default)]
pub struct PlaySession {
    /// 转码中间产物 `.part`（仅边转边播分支有）。停止/切换时删除残留。
    pub part_path: Option<PathBuf>,
    /// 后台 ffmpeg 子进程（仅边转边播分支有）。停止/切换时 kill+wait。
    pub child: Option<Child>,
}

/// 当前播放会话（同一时刻只播一个视频）。
#[derive(Default)]
pub struct PlayerState(pub Mutex<Option<PlaySession>>);

/// 本地视频 HTTP server 的端口（setup 时启动、存入）。
pub struct HttpServerState {
    pub port: u16,
}

/// player_open 返回给前端：视频 http URL（127.0.0.1:port/文件名）+ 时长。
/// progressive=true 表示边转边播（前端应等 transcode-progress 达阈值再起播）；
/// false 表示完整产物秒开复用。epoch 用于前端过滤进度事件。
#[derive(Serialize)]
pub struct PlayerInfo {
    pub src: String,
    pub duration: f64,
    pub subtitle: Option<String>,
    pub progressive: bool,
    pub epoch: u64,
}

/// 转码进度事件负载（emit 到前端 "transcode-progress"）。
#[derive(Serialize, Clone)]
struct TranscodeProgress {
    epoch: u64,
    /// 已转码到的秒数（out_time_ms/1000）。前端据此判断可起播、限制 seek 上界。
    ok_seconds: f64,
    /// 转码是否已完成（成功 rename）。
    done: bool,
    /// 转码是否失败（ffmpeg 非零退出/rename 失败）。
    failed: bool,
}

/// 按 category/category_path/title 推导字幕明文路径，存在则返回绝对路径。
fn resolve_subtitle(app_data: &str, category: &str, category_path: &str, title: &str) -> Option<String> {
    let abs = crate::library::paths::subtitle_abs_path(app_data, category, category_path, title);
    if std::path::Path::new(&abs).is_file() { Some(abs) } else { None }
}

/// 停止某会话的后台转码：kill+wait 子进程，删残留 `.part`。在锁外调用（child 已 take 出）。
fn stop_session(mut child: Option<Child>, part: Option<PathBuf>) {
    if let Some(mut c) = child.take() {
        let _ = c.kill();
        let _ = c.wait();
    }
    if let Some(p) = part {
        let _ = std::fs::remove_file(&p);
    }
}

/// 打开视频：完整产物秒开复用，否则后台 spawn ffmpeg 边转边播，立即返回。
/// 边转边播时后台线程读 ffmpeg stderr 进度，emit "transcode-progress"，成功后 rename `.part`→最终名。
/// src 是本地 HTTP server 的 URL（支持 Range 流式，GB 视频不 OOM）。
#[tauri::command(rename_all = "camelCase")]
pub fn player_open(
    app: tauri::AppHandle,
    db: tauri::State<crate::db::Db>,
    state: tauri::State<PlayerState>,
    http: tauri::State<HttpServerState>,
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

    // 先停旧会话（take 出 child/part 后锁外 kill，缩短临界区）
    let prev = state.0.lock().unwrap().take();
    if let Some(mut s) = prev {
        stop_session(s.child.take(), s.part_path.take());
    }

    let status = transcode::start_remux(&cache_dir, &abs)?;
    let epoch = EPOCH.fetch_add(1, Ordering::SeqCst) + 1;

    let (final_path, duration, progressive, session) = match status {
        transcode::TranscodeStatus::Ready(path, duration) => {
            let final_path = path.to_string_lossy().into_owned();
            let session = PlaySession { part_path: None, child: None };
            (final_path, duration, false, session)
        }
        transcode::TranscodeStatus::Started(mut handle, duration) => {
            let final_path = handle.final_path.to_string_lossy().into_owned();
            // 登记估算总长：Safari/WKWebView 拒绝增长 .part 的 `.../*` 响应，需给确定总长。
            // 视频 -c:v copy 不变、音频转 AAC 通常更小，故源文件大小是安全的偏大估算。
            if let Some(fname) = handle.final_path.file_name().and_then(|s| s.to_str()) {
                let est = std::fs::metadata(&abs).map(|m| m.len()).unwrap_or(0);
                if est > 0 {
                    httpserver::set_estimated_total(fname, est);
                }
            }
            // 取出 stderr 起后台进度线程；child handle 存入 session 供 kill。
            let stderr = handle.child.stderr.take();
            let part = handle.part_path.clone();
            let final_p = handle.final_path.clone();
            let cache = cache_dir.clone();
            let app_h = app.clone();
            if let Some(stderr) = stderr {
                std::thread::spawn(move || {
                    progress_worker(app_h, stderr, epoch, part, final_p, cache);
                });
            }
            let session = PlaySession {
                part_path: Some(handle.part_path.clone()),
                child: Some(handle.child),
            };
            (final_path, duration, true, session)
        }
    };

    let file_name = std::path::Path::new(&final_path)
        .file_name()
        .and_then(|s| s.to_str())
        .ok_or_else(|| crate::error::AppError::Other("bad cache file name".into()))?;
    let src = format!("http://127.0.0.1:{}/{}", http.port, file_name);
    let app_data_str = app_data.to_string_lossy().to_string();
    let subtitle = resolve_subtitle(&app_data_str, &category, &category_path, &title);
    *state.0.lock().unwrap() = Some(session);
    Ok(PlayerInfo { src, duration, subtitle, progressive, epoch })
}

/// 后台线程：读 ffmpeg stderr 的 `-progress pipe:2` 输出，解析 out_time_ms，
/// emit "transcode-progress"。ffmpeg 进程结束后按退出码判定成功，成功则 rename `.part`→最终名。
fn progress_worker(
    app: tauri::AppHandle,
    stderr: std::process::ChildStderr,
    epoch: u64,
    part: PathBuf,
    final_path: PathBuf,
    cache_dir: PathBuf,
) {
    use std::io::{BufRead, BufReader};
    let reader = BufReader::new(stderr);
    let mut ok_seconds = 0.0f64;
    let mut saw_end = false;
    for line in reader.lines().map_while(Result::ok) {
        // -progress pipe:2 输出形如 `out_time_ms=1234567` / `progress=continue|end`
        if let Some(v) = line.strip_prefix("out_time_ms=") {
            if let Ok(us) = v.trim().parse::<u64>() {
                ok_seconds = us as f64 / 1_000_000.0; // 单位是微秒
                // 转码进度（供前端显示百分比）。WKWebView 需转码完成才播，故起播由 done 事件触发。
                let _ = app.emit(
                    "transcode-progress",
                    TranscodeProgress { epoch, ok_seconds, done: false, failed: false },
                );
            }
        } else if line.strip_prefix("progress=") == Some("end") {
            saw_end = true;
        }
    }
    // stderr 关闭（进程即将/已退出）。判定成功并 finalize。
    let success = saw_end && part.metadata().map(|m| m.len() > 0).unwrap_or(false);
    eprintln!("[DIAG worker-end] saw_end={saw_end} part字节={} success={success}",
        part.metadata().map(|m| m.len()).unwrap_or(0));
    if success && transcode::finalize_remux(&part, &final_path, &cache_dir).is_ok() {
        let _ = app.emit(
            "transcode-progress",
            TranscodeProgress { epoch, ok_seconds, done: true, failed: false },
        );
    } else {
        // 失败：清理半成品 .part
        let _ = std::fs::remove_file(&part);
        let _ = app.emit(
            "transcode-progress",
            TranscodeProgress { epoch, ok_seconds, done: false, failed: true },
        );
    }
}

/// 停止播放：kill 后台转码、清残留 `.part`、清状态（退出播放模式时调用）。
/// 完整缓存产物不删，磁盘由 LRU 管。
#[tauri::command]
pub fn player_stop(state: tauri::State<PlayerState>) -> AppResult<()> {
    let prev = state.0.lock().unwrap().take();
    if let Some(mut s) = prev {
        stop_session(s.child.take(), s.part_path.take());
    }
    Ok(())
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
