mod comic;
mod db;
mod error;
mod launcher;
mod library;
mod normalize;
mod player;
mod settings;

use db::Db;
use error::AppResult;
use library::model::{MediaItem, MediaKind};
use tauri::Manager;

#[tauri::command]
fn set_root(db: tauri::State<Db>, kind: String, path: String) -> AppResult<()> {
    settings::set(&db, &format!("{kind}_root"), &path)
}

#[tauri::command]
fn get_root(db: tauri::State<Db>, kind: String) -> AppResult<Option<String>> {
    settings::get(&db, &format!("{kind}_root"))
}

#[tauri::command]
fn scan_root(db: tauri::State<Db>, kind: String) -> AppResult<usize> {
    let k = MediaKind::from_kind_str(&kind)?;
    let root = settings::get(&db, &format!("{kind}_root"))?
        .ok_or_else(|| error::AppError::Invalid(format!("{kind} root not set")))?;
    let items = match k {
        MediaKind::Video => library::scanner::scan_videos(std::path::Path::new(&root)),
        MediaKind::Comic => library::scanner::scan_comics(std::path::Path::new(&root)),
        MediaKind::Game => library::scanner::scan_games(std::path::Path::new(&root)),
    };
    library::upsert_items(&db, &items)
}

#[tauri::command]
fn list_media(db: tauri::State<Db>, kind: String) -> AppResult<Vec<MediaItem>> {
    let k = MediaKind::from_kind_str(&kind)?;
    library::list_items(&db, k)
}

#[tauri::command(rename_all = "camelCase")]
fn set_comic_page(db: tauri::State<Db>, item_id: i64, page: i64) -> AppResult<()> {
    let conn = db.0.lock().unwrap();
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;
    conn.execute(
        "INSERT INTO watch_state(item_id,comic_page,last_opened_at) VALUES(?1,?2,?3)
         ON CONFLICT(item_id) DO UPDATE SET comic_page=excluded.comic_page, last_opened_at=excluded.last_opened_at",
        rusqlite::params![item_id, page, now],
    )
    .map_err(|e| error::AppError::Db(e.to_string()))?;
    Ok(())
}

#[tauri::command(rename_all = "camelCase")]
fn get_comic_page(db: tauri::State<Db>, item_id: i64) -> AppResult<i64> {
    let conn = db.0.lock().unwrap();
    conn.query_row(
        "SELECT comic_page FROM watch_state WHERE item_id=?1",
        rusqlite::params![item_id],
        |r| r.get(0),
    )
    .or_else(|e| match e {
        rusqlite::Error::QueryReturnedNoRows => Ok(0),
        o => Err(error::AppError::Db(o.to_string())),
    })
}

#[tauri::command(rename_all = "camelCase")]
fn set_video_pos(db: tauri::State<Db>, item_id: i64, secs: f64) -> AppResult<()> {
    let conn = db.0.lock().unwrap();
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;
    conn.execute(
        "INSERT INTO watch_state(item_id,position_secs,last_opened_at) VALUES(?1,?2,?3)
         ON CONFLICT(item_id) DO UPDATE SET position_secs=excluded.position_secs, last_opened_at=excluded.last_opened_at",
        rusqlite::params![item_id, secs, now],
    )
    .map_err(|e| error::AppError::Db(e.to_string()))?;
    Ok(())
}

#[tauri::command(rename_all = "camelCase")]
fn get_video_pos(db: tauri::State<Db>, item_id: i64) -> AppResult<f64> {
    let conn = db.0.lock().unwrap();
    conn.query_row(
        "SELECT position_secs FROM watch_state WHERE item_id=?1",
        rusqlite::params![item_id],
        |r| r.get(0),
    )
    .or_else(|e| match e {
        rusqlite::Error::QueryReturnedNoRows => Ok(0.0),
        o => Err(error::AppError::Db(o.to_string())),
    })
}

/// 创建承载 mpv 渲染的子窗口并初始化播放器。
///
/// macOS 上 mpv 的 `wid` 期望 NSView 指针（不是 NSWindow）。这里取 Tauri
/// 子窗口的 `ns_view()`（`*mut c_void`）转 i64 传给 mpv 的 wid。
///
/// 注意 / 局限：Tauri 的 WebviewWindow 其内容视图被 WKWebView 占据，
/// mpv 直接渲染到该 NSView 时可能被 WebView 内容遮挡或冲突（见报告）。
/// 因此本窗口以 `about:blank` 空白页承载，尽量减少 WebView 占用。
#[tauri::command]
fn open_player_window(
    app: tauri::AppHandle,
    state: tauri::State<player::PlayerState>,
) -> AppResult<()> {
    use tauri::{Manager, WebviewWindowBuilder};
    let win = match app.get_webview_window("mpv") {
        Some(w) => w,
        None => WebviewWindowBuilder::new(&app, "mpv", tauri::WebviewUrl::App("about:blank".into()))
            .title("player")
            .decorations(false)
            .inner_size(960.0, 540.0)
            .build()
            .map_err(|e| error::AppError::Other(e.to_string()))?,
    };
    #[cfg(target_os = "macos")]
    let wid = win
        .ns_view()
        .map_err(|e| error::AppError::Other(e.to_string()))? as i64;
    #[cfg(target_os = "windows")]
    let wid = win
        .hwnd()
        .map_err(|e| error::AppError::Other(e.to_string()))?
        .0 as i64;
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    let wid: i64 = 0;
    *state.0.lock().unwrap() = Some(player::mpv::Player::new(wid)?);
    Ok(())
}

#[tauri::command]
fn player_fullscreen(state: tauri::State<player::PlayerState>, on: bool) -> AppResult<()> {
    let g = state.0.lock().unwrap();
    let p = g
        .as_ref()
        .ok_or_else(|| error::AppError::Invalid("player not initialized".into()))?;
    p.set_fullscreen(on)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let dir = app.path().app_data_dir().expect("app data dir");
            std::fs::create_dir_all(&dir).ok();
            let db = Db::open(&dir.join("collector.sqlite")).expect("open db");
            app.manage(db);
            app.manage(player::PlayerState::default());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            set_root,
            get_root,
            scan_root,
            list_media,
            comic::comic_pages,
            comic::comic_page,
            comic::comic_cover,
            set_comic_page,
            get_comic_page,
            set_video_pos,
            get_video_pos,
            open_player_window,
            player_fullscreen,
            player::player_init,
            player::player_load,
            player::player_pause,
            player::player_seek,
            player::player_seek_to,
            player::player_volume,
            player::player_progress,
            launcher::launch_game,
            normalize::normalize_subtitles
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
