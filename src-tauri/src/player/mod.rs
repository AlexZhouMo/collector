//! 视频播放：H.264 MKV remux 成 MP4（放应用数据缓存目录），前端 <video> 经
//! 本地 HTTP server（支持 Range/206 流式）播放。缓存产物不随停止删除，磁盘由 LRU 管理。
pub mod httpserver;
pub mod transcode;

use crate::error::AppResult;
use rusqlite::OptionalExtension;
use serde::Serialize;
use std::sync::Mutex;
use tauri::Manager;

/// 当前播放的 mp4 缓存路径（同一时刻只播一个视频）。
#[derive(Default)]
pub struct PlayerState(pub Mutex<Option<String>>);

/// 本地视频 HTTP server 的端口（setup 时启动、存入）。
pub struct HttpServerState {
    pub port: u16,
}

/// player_open 返回给前端：视频 http URL（127.0.0.1:port/文件名）+ 时长。
#[derive(Serialize)]
pub struct PlayerInfo {
    pub src: String,
    pub duration: f64,
    pub subtitle: Option<String>,
}

/// 按 (category_path, title) 查该条视频的字幕相对路径，转成 app_data 下绝对路径。
/// 查不到行、字段为 NULL 或空串 → Ok(None)。
fn resolve_subtitle(
    db: &crate::db::Db,
    category_path: &str,
    title: &str,
    app_data: &str,
) -> AppResult<Option<String>> {
    let conn = db.0.lock().unwrap();
    let rel: Option<String> = conn
        .query_row(
            "SELECT subtitle_path FROM media WHERE category_path=?1 AND title=?2",
            rusqlite::params![category_path, title],
            |r| r.get(0),
        )
        .optional()
        .map_err(|e| crate::error::AppError::Db(e.to_string()))?
        .flatten();
    let abs = match rel {
        Some(s) if !s.trim().is_empty() => {
            Some(crate::library::paths::appdata_to_absolute(&s, app_data))
        }
        _ => None,
    };
    Ok(abs)
}

/// 打开视频：remux 成缓存目录下 mp4（秒级，等完成），返回 {src, duration}。
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
    let cache_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| crate::error::AppError::Other(format!("app_data_dir: {e}")))?
        .join("video_cache");
    let (abs_path, duration) = transcode::remux(&cache_dir, &abs)?;
    // 取产物文件名，拼成 http URL 给 <video>（server 只服务 video_cache 目录）
    let file_name = std::path::Path::new(&abs_path)
        .file_name()
        .and_then(|s| s.to_str())
        .ok_or_else(|| crate::error::AppError::Other("bad cache file name".into()))?;
    let src = format!("http://127.0.0.1:{}/{}", http.port, file_name);
    let app_data_str = app
        .path()
        .app_data_dir()
        .map_err(|e| crate::error::AppError::Other(format!("app_data_dir: {e}")))?
        .to_string_lossy()
        .to_string();
    let subtitle = resolve_subtitle(&db, &category_path, &title, &app_data_str)?;
    *state.0.lock().unwrap() = Some(abs_path);
    Ok(PlayerInfo { src, duration, subtitle })
}

/// 停止播放：清状态（退出播放模式时调用）。不删缓存产物，磁盘由 LRU 管。
#[tauri::command]
pub fn player_stop(state: tauri::State<PlayerState>) -> AppResult<()> {
    *state.0.lock().unwrap() = None;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Db;

    fn seed(db: &Db, cpath: &str, title: &str, sub: Option<&str>) {
        let conn = db.0.lock().unwrap();
        conn.execute(
            "INSERT INTO media (category,category_path,title,subtitle_path) VALUES ('电影',?1,?2,?3)",
            rusqlite::params![cpath, title, sub],
        ).unwrap();
    }

    #[test]
    fn resolve_subtitle_found_relative_to_abs() {
        let db = Db::open_in_memory().unwrap();
        seed(&db, "科幻/星战", "星战", Some("subtitles/sub_a.ass"));
        let got = resolve_subtitle(&db, "科幻/星战", "星战", "/app").unwrap();
        assert_eq!(got, Some("/app/subtitles/sub_a.ass".to_string()));
    }

    #[test]
    fn resolve_subtitle_null_or_empty_is_none() {
        let db = Db::open_in_memory().unwrap();
        seed(&db, "科幻/沙丘", "沙丘", None);
        seed(&db, "科幻/降临", "降临", Some(""));
        assert_eq!(resolve_subtitle(&db, "科幻/沙丘", "沙丘", "/app").unwrap(), None);
        assert_eq!(resolve_subtitle(&db, "科幻/降临", "降临", "/app").unwrap(), None);
    }

    #[test]
    fn resolve_subtitle_missing_row_is_none() {
        let db = Db::open_in_memory().unwrap();
        assert_eq!(resolve_subtitle(&db, "不存在", "无此片", "/app").unwrap(), None);
    }
}
