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

/// 把 mpv 渲染 NSView 作为主窗 WebView 的子视图嵌入，定位到内容面板视频区
/// 矩形（DOM 逻辑坐标，相对视口左上）。同窗口子视图，天然跟随窗口移动缩放。
///
/// NSView 操作必须在主线程执行，故用 run_on_main_thread + channel 同步取回
/// mpv 子视图指针，再据此创建 Player。
#[tauri::command(rename_all = "camelCase")]
fn player_embed(
    app: tauri::AppHandle,
    player_state: tauri::State<player::PlayerState>,
    embed_state: tauri::State<player::EmbedState>,
    x: f64,
    y: f64,
    width: f64,
    height: f64,
) -> AppResult<()> {
    use tauri::Manager;
    // 已嵌入则只更新 bounds（复用），不重建
    if embed_state.0.lock().unwrap().is_some() {
        return player_set_bounds(app, embed_state, x, y, width, height);
    }
    let main = app
        .get_webview_window("main")
        .ok_or_else(|| error::AppError::Other("main window not found".into()))?;
    let parent = main
        .ns_view()
        .map_err(|e| error::AppError::Other(e.to_string()))? as usize;

    let (tx, rx) = std::sync::mpsc::channel::<usize>();
    let w = width.max(1.0);
    let h = height.max(1.0);
    app.run_on_main_thread(move || {
        // Safety: parent 是主窗 WebView content view，存活；在主线程执行。
        let mpv_view =
            unsafe { player::embed_macos::create_mpv_view(parent as *mut _, x, y, w, h) } as usize;
        let _ = tx.send(mpv_view);
    })
    .map_err(|e| error::AppError::Other(e.to_string()))?;
    let mpv_view = rx
        .recv()
        .map_err(|e| error::AppError::Other(format!("embed recv: {e}")))?;

    *embed_state.0.lock().unwrap() = Some(player::EmbedHandle { mpv_view, parent });
    let mut guard = player_state.0.lock().unwrap();
    if guard.is_none() {
        *guard = Some(player::mpv::Player::new(mpv_view as i64)?);
    }
    Ok(())
}

/// 更新 mpv 子视图 frame（内容区 resize / 主窗移动时保持贴合）。
#[tauri::command(rename_all = "camelCase")]
fn player_set_bounds(
    app: tauri::AppHandle,
    embed_state: tauri::State<player::EmbedState>,
    x: f64,
    y: f64,
    width: f64,
    height: f64,
) -> AppResult<()> {
    let handle = match *embed_state.0.lock().unwrap() {
        Some(h) => h,
        None => return Ok(()),
    };
    let w = width.max(1.0);
    let h = height.max(1.0);
    app.run_on_main_thread(move || unsafe {
        player::embed_macos::set_mpv_view_frame(
            handle.mpv_view as *mut _,
            handle.parent as *mut _,
            x,
            y,
            w,
            h,
        );
    })
    .map_err(|e| error::AppError::Other(e.to_string()))?;
    Ok(())
}

/// 移除 mpv 子视图（退出播放时）。
#[tauri::command]
fn player_close_window(
    app: tauri::AppHandle,
    embed_state: tauri::State<player::EmbedState>,
) -> AppResult<()> {
    let handle = embed_state.0.lock().unwrap().take();
    if let Some(h) = handle {
        app.run_on_main_thread(move || unsafe {
            player::embed_macos::remove_mpv_view(h.mpv_view as *mut _);
        })
        .map_err(|e| error::AppError::Other(e.to_string()))?;
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
            app.manage(player::EmbedState::default());
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
            player_embed,
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
