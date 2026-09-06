mod comic;
mod db;
mod error;
mod library;
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
        // game 为后续阶段接入的合法占位
        MediaKind::Game => Vec::new(),
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
            get_comic_page
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
