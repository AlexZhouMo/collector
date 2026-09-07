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
        // 视频改用 scan_videos_all（按分类分别配置目录），此处不再处理
        MediaKind::Video => {
            return Err(error::AppError::Invalid(
                "use scan_videos_all for video".into(),
            ))
        }
        MediaKind::Comic => library::scanner::scan_comics(std::path::Path::new(&root)),
        MediaKind::Game => library::scanner::scan_games(std::path::Path::new(&root)),
    };
    library::upsert_items(&db, &items)
}

/// 分别扫描电影/动漫/电视剧三个目录（各自 settings key），分类由目录决定。
/// 未设置的目录跳过。返回本次入库的条目总数。
#[tauri::command]
fn scan_videos_all(db: tauri::State<Db>) -> AppResult<usize> {
    // (settings key 前缀, 分类名)
    const VIDEO_DIRS: &[(&str, &str)] = &[
        ("video_movie", "电影"),
        ("video_anime", "动漫"),
        ("video_tv", "电视剧"),
    ];
    let mut all = Vec::new();
    for (key, category) in VIDEO_DIRS {
        if let Some(root) = settings::get(&db, &format!("{key}_root"))? {
            if !root.is_empty() {
                all.extend(library::scanner::scan_videos(
                    std::path::Path::new(&root),
                    category,
                ));
            }
        }
    }
    library::upsert_items(&db, &all)
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

/// 创建/复用承载 mpv 渲染的子窗口，定位到内容面板视频区矩形（逻辑坐标）。
/// 设为主窗子窗口，使层级与移动跟随父窗。
///
/// macOS 上 mpv 的 `wid` 期望 NSView 指针（不是 NSWindow）。这里取 Tauri
/// 子窗口的 `ns_view()`（`*mut c_void`）转 i64 传给 mpv 的 wid。
///
/// 父子绑定说明：Tauri 2.11 的 WebviewWindow 没有运行时 `set_parent`，
/// 父窗口只能在构建时通过 `WebviewWindowBuilder::parent(&main)` 设置
/// （macOS = addChildWindow，Windows = owned window），因此这里改在
/// build 前绑定，效果等价于原计划的 set_parent。
#[tauri::command(rename_all = "camelCase")]
fn open_player_window(
    app: tauri::AppHandle,
    state: tauri::State<player::PlayerState>,
    x: f64,
    y: f64,
    width: f64,
    height: f64,
) -> AppResult<()> {
    use tauri::{LogicalPosition, LogicalSize, Manager, WebviewWindowBuilder};
    let win = match app.get_webview_window("mpv") {
        Some(w) => w,
        None => {
            // 不绑定为子窗口：子窗口 set_position 坐标相对父窗，会与前端传的
            // 屏幕绝对坐标口径冲突导致错位。改为独立顶层窗口 + 屏幕绝对坐标，
            // 窗口移动/缩放由前端 onMoved + ResizeObserver 调 player_set_bounds 补偿。
            // transparent 消除 about:blank 的 WebView 白底（mpv 渲染层在其下方）。
            WebviewWindowBuilder::new(
                &app,
                "mpv",
                tauri::WebviewUrl::App("about:blank".into()),
            )
            .title("player")
            .decorations(false)
            .transparent(true)
            .focused(false)
            .inner_size(width.max(1.0), height.max(1.0))
            .build()
            .map_err(|e| error::AppError::Other(e.to_string()))?
        }
    };
    win.set_position(LogicalPosition::new(x, y))
        .map_err(|e| error::AppError::Other(e.to_string()))?;
    win.set_size(LogicalSize::new(width.max(1.0), height.max(1.0)))
        .map_err(|e| error::AppError::Other(e.to_string()))?;
    let _ = win.show();
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
    let mut guard = state.0.lock().unwrap();
    if guard.is_none() {
        *guard = Some(player::mpv::Player::new(wid)?);
    }
    Ok(())
}

/// 更新 mpv 窗口位置/尺寸（前端在内容区 resize / 主窗移动时调用保持覆盖）。
#[tauri::command(rename_all = "camelCase")]
fn player_set_bounds(
    app: tauri::AppHandle,
    x: f64,
    y: f64,
    width: f64,
    height: f64,
) -> AppResult<()> {
    use tauri::{LogicalPosition, LogicalSize, Manager};
    if let Some(win) = app.get_webview_window("mpv") {
        win.set_position(LogicalPosition::new(x, y))
            .map_err(|e| error::AppError::Other(e.to_string()))?;
        win.set_size(LogicalSize::new(width.max(1.0), height.max(1.0)))
            .map_err(|e| error::AppError::Other(e.to_string()))?;
    }
    Ok(())
}

/// 隐藏 mpv 窗口（退出播放时立即调用，UI 观感上瞬时消失）。
#[tauri::command]
fn player_close_window(app: tauri::AppHandle) -> AppResult<()> {
    use tauri::Manager;
    if let Some(win) = app.get_webview_window("mpv") {
        let _ = win.hide();
    }
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
            scan_videos_all,
            list_media,
            comic::comic_pages,
            comic::comic_page,
            comic::comic_cover,
            set_comic_page,
            get_comic_page,
            set_video_pos,
            get_video_pos,
            open_player_window,
            player_set_bounds,
            player_close_window,
            player_fullscreen,
            player::player_load,
            player::player_pause,
            player::player_seek,
            player::player_seek_to,
            player::player_volume,
            player::player_progress,
            player::player_stop,
            player::player_close,
            launcher::launch_game,
            normalize::normalize_subtitles,
            normalize::comic_pack::normalize_comic
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
