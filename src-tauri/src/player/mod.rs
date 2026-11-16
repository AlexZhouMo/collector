//! 视频播放：H.264 MKV 用 ffmpeg 转成 HLS（`.m3u8` + `.ts` segments），
//! 前端 <video> + hls.js 走 MSE 播放（Chromium 无原生 HLS）。
//! - AC-3 音轨在 Windows 上被静音——转 AAC；其他音频保持 copy。
//! - HLS 段边转边写，首段（~2s）就绪即可起播；缓存复用，磁盘由 LRU 管理。
//! - 文件从 webview 到磁盘的通道：见 [lib.rs](../lib.rs.html) 的 `register_uri_scheme_protocol("hls", ...)`
//!   自定义 URI 协议 handler（Tauri asset 协议对 fetch 返 404，本地 HTTP server 被
//!   WebView2 拦截，所以自建 URI 协议）。
pub mod transcode;
pub mod ffmpeg_paths;

use crate::error::{AppError, AppResult};
use serde::Serialize;
use std::collections::VecDeque;
use std::path::PathBuf;
use std::process::Child;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tauri::{Emitter, Manager};

/// 当前播放会话：转码组前缀 + 后台 ffmpeg 子进程。同一时刻只播一个视频。
/// 完整产物播放（`Ready`）时 child/prefix 均为 None。
#[derive(Default)]
pub struct PlaySession {
    /// 后台 ffmpeg 子进程（仅转码中分支有）。停止/切换时直接 kill。
    pub child: Option<Child>,
    /// 缓存组前缀 `collector_HASH`（仅转码中分支有）。停止时用来清 .tmp 残留。
    pub cache_prefix: Option<String>,
}

/// 播放器全局状态。
#[derive(Default)]
pub struct PlayerState {
    /// 当前播放会话（同一时刻只播一个视频）。
    pub session: Mutex<Option<PlaySession>>,
    /// 串行化 `player_open`：它跑在线程池上（不阻塞界面），快速连点两个视频时
    /// 后一次必须等前一次把会话登记好，才能把前一个 ffmpeg 停掉，不留孤儿进程。
    opening: Mutex<()>,
}

/// player_open 返回给前端：HLS playlist 的文件名（前端拼 `http://hls.localhost/文件名`）+ 时长。
#[derive(Serialize)]
pub struct PlayerInfo {
    pub src: String,
    pub duration: f64,
    pub subtitle: Option<String>,
    /// true = 后台仍在转码（前端显示缓存进度）；false = 完整缓存直接播。
    pub transcoding: bool,
    /// 打开时已转好的时长（秒）。续转时 > 0；完整缓存 = duration。
    pub transcoded: f64,
}

/// `player-transcode` 事件载荷：后台转码进度。
#[derive(Serialize, Clone)]
struct TranscodeProgress {
    /// playlist 文件名，前端据此丢弃不属于当前视频的事件
    src: String,
    /// 已转好的时长（秒，含续转前已有部分）
    seconds: f64,
    /// 转码已完整结束
    done: bool,
}

/// 按 category/category_path/title 推导字幕明文路径，存在则返回绝对路径。
fn resolve_subtitle(app_data: &str, category: &str, category_path: &str, title: &str) -> Option<String> {
    let abs = crate::library::paths::subtitle_abs_path(app_data, category, category_path, title);
    if std::path::Path::new(&abs).is_file() { Some(abs) } else { None }
}

/// 停止会话：kill ffmpeg，然后按需清残留。
///
/// 直接 kill 而不是"优雅退出"：ffmpeg 以 `-nostdin` 启动，往 stdin 写 `q` 会被忽略
/// （实测 4 秒内不退出），旧实现因此每次返回都白等 3 秒再强杀。而真正的优雅收尾反而有害——
/// HLS muxer 收尾时会写 `#EXT-X-ENDLIST`，让半截缓存被当成完整缓存。kill 是安全的：
/// 段和 playlist 都先写 `.tmp` 再 rename，磁盘上的 `.ts` / `.m3u8` 始终是完整的。
/// - `preserve_partial=true`（切换视频/返回）：只清 `.tmp`，保留 `.m3u8` + `.ts` 供下次续转
/// - `preserve_partial=false`（cache_clear）：整组删
fn stop_session(session: PlaySession, cache_dir: &std::path::Path, preserve_partial: bool) {
    if let Some(mut c) = session.child {
        let _ = c.kill();
        let _ = c.wait();
    }
    let prefix = match session.cache_prefix { Some(p) => p, None => return };
    let rd = match std::fs::read_dir(cache_dir) { Ok(r) => r, Err(_) => return };
    for e in rd.filter_map(|e| e.ok()) {
        let name = match e.file_name().into_string() { Ok(n) => n, Err(_) => continue };
        if !name.starts_with(&prefix) { continue; }
        let is_tmp = name.ends_with(".tmp");
        let is_product = name.ends_with(".m3u8") || name.ends_with(".ts");
        if is_tmp || (!preserve_partial && is_product) {
            let _ = std::fs::remove_file(e.path());
        }
    }
}

/// 等 ffmpeg 把首个段写进 playlist 再让前端去加载。
///
/// 这是"首次打开没反应"的根因修复：旧实现 spawn 完立即返回，前端马上请求 playlist，
/// 而 ffmpeg 还没来得及创建它 → 404。hls.js 对 4xx 不重试，直接 fatal，界面就停死了
/// （ffmpeg 却在后台转完，所以第二次打开命中完整缓存、一切正常）。
///
/// `base_segments`：启动前 playlist 已有的段数（续转时 > 0）。必须等到**新段**出现才算
/// 这次 ffmpeg 真的跑起来了——否则续转进程一启动就挂掉也会被当成成功。
fn wait_first_segment(
    playlist: &std::path::Path,
    base_segments: usize,
    child: &mut Child,
    stderr_tail: &Mutex<VecDeque<String>>,
) -> AppResult<()> {
    let deadline = Instant::now() + Duration::from_secs(30);
    let produced = || transcode::playlist_segment_count(playlist) > base_segments;
    loop {
        if produced() {
            return Ok(());
        }
        if let Ok(Some(status)) = child.try_wait() {
            // 进程已退出：给 stderr 线程一点时间收完，再看一次产物。
            // 正常收尾（写了 ENDLIST）也算成功——续转时剩余内容可能不足一个新段。
            std::thread::sleep(Duration::from_millis(150));
            if produced() || (status.success() && transcode::playlist_is_complete(playlist)) {
                return Ok(());
            }
            let tail: Vec<String> = stderr_tail.lock().unwrap().iter().cloned().collect();
            return Err(AppError::Other(format!(
                "转码未能启动（ffmpeg {status}）。\n\n{}",
                tail.join("\n")
            )));
        }
        if Instant::now() >= deadline {
            return Err(AppError::Other(
                "等待首个视频片段超时（30 秒）。源文件可能损坏，或所在磁盘响应过慢。".into(),
            ));
        }
        std::thread::sleep(Duration::from_millis(25));
    }
}

/// 打开视频：完整 HLS 缓存复用则直接返回 playlist 文件名；否则后台 spawn ffmpeg
/// 增量写 HLS segments，**等首段就绪后**返回 playlist 文件名——hls.js 会周期性 refetch
/// 直到看到 `#EXT-X-ENDLIST`。前端拼 `http://hls.localhost/<文件名>` 走 lib.rs 里的
/// 自定义 URI 协议 handler。
///
/// `async`：在线程池上执行。同步命令跑在主线程，探测/等首段期间整个窗口会卡住。
/// 过程中通过 `player-stage` 事件上报阶段（check/probe/transcode/segment）。
#[tauri::command(async, rename_all = "camelCase")]
pub fn player_open(
    app: tauri::AppHandle,
    db: tauri::State<crate::db::Db>,
    state: tauri::State<PlayerState>,
    category: String,
    category_path: String,
    title: String,
) -> AppResult<PlayerInfo> {
    let _opening = state.opening.lock().unwrap_or_else(|e| e.into_inner());
    let root = crate::settings::get(&db, crate::library::paths::video_root_key(&category))?
        .unwrap_or_default();
    let abs = crate::library::paths::video_abs_path(&root, &category_path, &title);
    if abs.is_empty() || !std::path::Path::new(&abs).is_file() {
        return Err(AppError::Other("视频文件不存在".into()));
    }
    let app_data = app
        .path()
        .app_data_dir()
        .map_err(|e| AppError::Other(format!("app_data_dir: {e}")))?;
    let cache_dir = app_data.join("video_cache");

    // 先停旧会话（保留部分缓存供续转/复用）
    let prev = state.session.lock().unwrap().take();
    if let Some(s) = prev {
        stop_session(s, &cache_dir, true);
    }

    let stage = |s: &str| {
        let _ = app.emit("player-stage", s);
    };
    // 先尝试续转；续转起不来（半截缓存有问题等）就作废缓存、从头再转一次。
    let mut allow_resume = true;
    let (playlist_path, duration, session, transcoding) = loop {
        let status = transcode::start_remux(&cache_dir, &abs, allow_resume, &stage)?;
        match status {
        transcode::TranscodeStatus::Ready(path, duration) => {
            break (path, duration, PlaySession::default(), false);
        }
        transcode::TranscodeStatus::Started(mut handle, duration) => {
            let src = file_name_of(&handle.final_path)?;
            // 后台线程：drain ffmpeg stderr、上报转码进度，进程结束时 LRU。
            let stderr_tail = Arc::new(Mutex::new(VecDeque::new()));
            if let Some(stderr) = handle.child.stderr.take() {
                let worker = FinalizeWorker {
                    app: app.clone(),
                    src,
                    final_path: handle.final_path.clone(),
                    cache_dir: cache_dir.clone(),
                    resume_offset: handle.resume_offset,
                    stderr_tail: stderr_tail.clone(),
                };
                std::thread::spawn(move || worker.run(stderr));
            }
            stage("segment");
            if let Err(e) = wait_first_segment(
                &handle.final_path,
                handle.base_segments,
                &mut handle.child,
                &stderr_tail,
            ) {
                let _ = handle.child.kill();
                let _ = handle.child.wait();
                if allow_resume && handle.base_segments > 0 {
                    allow_resume = false;
                    continue;
                }
                return Err(e);
            }
            let session = PlaySession {
                child: Some(handle.child),
                cache_prefix: Some(handle.cache_prefix),
            };
            break (handle.final_path, duration, session, true);
        }
        }
    };

    // 返回文件名——前端拼成 `http://hls.localhost/<file_name>`（自定义 URI 协议，
    // 见 lib.rs `register_uri_scheme_protocol("hls", ...)`）。
    // 不用 asset 协议：那个对 `<img>` 加载 OK，对 fetch/XHR 返 404（WebView2 里 media
    // 加载与 fetch 走不同路径）。hls.js 靠 fetch 加载 manifest/段，必须自定义协议。
    let src = file_name_of(&playlist_path)?;
    let transcoded = if transcoding {
        transcode::playlist_total_duration(&playlist_path)
    } else {
        duration
    };
    let app_data_str = app_data.to_string_lossy().to_string();
    let subtitle = resolve_subtitle(&app_data_str, &category, &category_path, &title);
    *state.session.lock().unwrap() = Some(session);
    Ok(PlayerInfo { src, duration, subtitle, transcoding, transcoded })
}

fn file_name_of(path: &std::path::Path) -> AppResult<String> {
    path.file_name()
        .and_then(|s| s.to_str())
        .map(str::to_string)
        .ok_or_else(|| AppError::Other("bad cache file name".into()))
}

/// 后台线程：drain ffmpeg stderr（含 `-progress pipe:2` 输出），把转码进度以
/// `player-transcode` 事件发给前端；进程结束后按退出标志判定成功，成功则 LRU 清理。
/// **失败/被 kill 时保留已转部分**（供下次续转）——只有用户手动 cache_clear 才整组删。
struct FinalizeWorker {
    app: tauri::AppHandle,
    src: String,
    final_path: PathBuf,
    cache_dir: PathBuf,
    resume_offset: f64,
    /// 最近几行非进度输出，转码起不来时拿来给用户看原因
    stderr_tail: Arc<Mutex<VecDeque<String>>>,
}

impl FinalizeWorker {
    fn run(self, stderr: std::process::ChildStderr) {
        use std::io::{BufRead, BufReader};
        let mut saw_end = false;
        let mut last_emit: Option<Instant> = None;
        // 按字节读行再 lossy 转码：路径里的中文在 ffmpeg 输出中未必是合法 UTF-8，
        // 用 `lines()` 会在第一处非法字节就停止 drain，进而把 ffmpeg 堵死在写 stderr 上。
        let mut reader = BufReader::new(stderr);
        let mut buf = Vec::new();
        loop {
            buf.clear();
            match reader.read_until(b'\n', &mut buf) {
                Ok(0) | Err(_) => break,
                Ok(_) => {}
            }
            let line = String::from_utf8_lossy(&buf);
            let line = line.trim();
            if let Some(us) = line.strip_prefix("out_time_us=") {
                // 节流：每 250ms 至多一条
                if last_emit.map_or(true, |t| t.elapsed() >= Duration::from_millis(250)) {
                    if let Ok(us) = us.parse::<f64>() {
                        last_emit = Some(Instant::now());
                        self.emit(self.resume_offset + us / 1_000_000.0, false);
                    }
                }
            } else if line == "progress=end" {
                saw_end = true;
            } else if !line.is_empty() && !is_progress_line(line) {
                let mut tail = self.stderr_tail.lock().unwrap();
                if tail.len() >= 8 {
                    tail.pop_front();
                }
                tail.push_back(line.to_string());
            }
        }
        // 成功完成（源到 EOF、playlist 追加了 ENDLIST）→ 通知前端 + LRU 清理。
        // 否则（被 kill / 崩溃）保留 .m3u8 + .ts 段供下次 `start_remux` 走续转分支。
        if saw_end && transcode::playlist_is_complete(&self.final_path) {
            self.emit(transcode::playlist_total_duration(&self.final_path), true);
            transcode::finalize_remux(&self.cache_dir);
        }
    }

    fn emit(&self, seconds: f64, done: bool) {
        let _ = self.app.emit(
            "player-transcode",
            TranscodeProgress { src: self.src.clone(), seconds, done },
        );
    }
}

/// `-progress` 输出的 `key=value` 行（frame=、fps=、out_time=、speed=…）：键只含小写字母/数字/下划线。
fn is_progress_line(line: &str) -> bool {
    match line.split_once('=') {
        Some((k, _)) => {
            !k.is_empty()
                && k.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
        }
        None => false,
    }
}

/// 停止播放：kill ffmpeg，**保留已转部分**——下次打开同视频走 `start_remux` 的续转分支，
/// 从上次停下的段号继续追加。`async`：不占主线程。
#[tauri::command(async)]
pub fn player_stop(app: tauri::AppHandle, state: tauri::State<PlayerState>) -> AppResult<()> {
    let cache_dir = video_cache_dir(&app)?;
    let prev = state.session.lock().unwrap().take();
    if let Some(s) = prev {
        stop_session(s, &cache_dir, true);
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
        .map_err(|e| AppError::Other(format!("app_data_dir: {e}")))?;
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

/// 清空视频缓存：先停当前会话（整组删），再删所有本模块管理的文件。
/// 返回删除的文件数。
#[tauri::command]
pub fn cache_clear(app: tauri::AppHandle, state: tauri::State<PlayerState>) -> AppResult<usize> {
    let cache_dir = video_cache_dir(&app)?;
    let prev = state.session.lock().unwrap().take();
    if let Some(s) = prev {
        stop_session(s, &cache_dir, false);
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

    #[test]
    fn progress_lines_are_not_kept_as_error_tail() {
        assert!(is_progress_line("out_time_us=2002000"));
        assert!(is_progress_line("progress=continue"));
        assert!(is_progress_line("speed= 143x"));
        assert!(!is_progress_line("[in#0 @ 000001] Error opening input: Invalid data"));
        assert!(!is_progress_line("Stream #0:0 -> #0:0 (copy)"));
    }

    #[test]
    fn stop_session_keeps_products_unless_clearing() {
        let tmp = tempfile::tempdir().unwrap();
        let fill = || {
            for n in [
                "collector_aa.m3u8",
                "collector_aa_00000.ts",
                "collector_aa_00001.ts.tmp",
                "collector_aa.m3u8.tmp",
                "collector_bb.m3u8",
            ] {
                std::fs::write(tmp.path().join(n), "x").unwrap();
            }
        };
        let names = || {
            let mut v: Vec<String> = std::fs::read_dir(tmp.path())
                .unwrap()
                .map(|e| e.unwrap().file_name().into_string().unwrap())
                .collect();
            v.sort();
            v
        };
        let session = || PlaySession { child: None, cache_prefix: Some("collector_aa".into()) };
        fill();
        stop_session(session(), tmp.path(), true);
        assert_eq!(names(), ["collector_aa.m3u8", "collector_aa_00000.ts", "collector_bb.m3u8"]);
        fill();
        stop_session(session(), tmp.path(), false);
        assert_eq!(names(), ["collector_bb.m3u8"]);
    }
}
