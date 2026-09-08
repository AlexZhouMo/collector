mod comic;
mod db;
mod error;
mod launcher;
mod library;
mod normalize;
mod player;
mod poster;
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
    // 重扫重建：先清空该 kind 旧记录再入库，清除磁盘上已删除的幽灵条目。
    library::replace_items(&db, k, &items)
}

/// 分别扫描电影/动漫/剧集三个目录（各自 settings key），分类由目录决定。
/// 未设置的目录跳过。返回本次入库的条目总数。
#[tauri::command]
fn scan_videos_all(db: tauri::State<Db>) -> AppResult<usize> {
    // (settings key 前缀, 分类名)
    const VIDEO_DIRS: &[(&str, &str)] = &[
        ("video_movie", "电影"),
        ("video_anime", "动漫"),
        ("video_tv", "剧集"),
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
    // 重扫重建：清空所有 video 记录再入库（三个目录合并为整个 video 库）。
    library::replace_items(&db, MediaKind::Video, &all)
}

/// 从 demo/subtitles 初始化导入视频库：扫电影/动漫/剧集三目录的 .ass 为条目，
/// 清库（含 id 序列重置）+ 排序顺序插入。demo_root 由前端传入（demo/subtitles 绝对路径）。
#[tauri::command(rename_all = "camelCase")]
fn init_from_demo(db: tauri::State<Db>, demo_root: String) -> AppResult<usize> {
    const CATS: &[(&str, &str)] = &[("电影", "电影"), ("动漫", "动漫"), ("剧集", "剧集")];
    let root = std::path::Path::new(&demo_root);
    let mut all = Vec::new();
    for (dir, category) in CATS {
        let cat_dir = root.join(dir);
        if cat_dir.is_dir() {
            all.extend(library::scanner::scan_videos_subs(&cat_dir, category));
        }
    }
    library::replace_items(&db, MediaKind::Video, &all)
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

/// 用前端传的字段构造一个 video ScannedItem（platform_ok=true, exec_path=None）。
fn build_video_item(
    category: String,
    category_path: String,
    title: String,
    path: String,
    subtitle_path: Option<String>,
    cover_path: Option<String>,
    description: Option<String>,
) -> library::scanner::ScannedItem {
    library::scanner::ScannedItem {
        kind: MediaKind::Video,
        category,
        category_path,
        title,
        path,
        subtitle_path,
        cover_path,
        description,
        platform_ok: true,
        exec_path: None,
    }
}

#[tauri::command(rename_all = "camelCase")]
fn media_update(
    db: tauri::State<Db>,
    id: i64,
    category: String,
    category_path: String,
    title: String,
    path: String,
    subtitle_path: Option<String>,
    cover_path: Option<String>,
    description: Option<String>,
) -> AppResult<()> {
    let it = build_video_item(category, category_path, title, path, subtitle_path, cover_path, description);
    library::update_item(&db, id, &it)
}

#[tauri::command(rename_all = "camelCase")]
fn media_create(
    db: tauri::State<Db>,
    category: String,
    category_path: String,
    title: String,
    path: String,
    subtitle_path: Option<String>,
    cover_path: Option<String>,
    description: Option<String>,
) -> AppResult<i64> {
    let it = build_video_item(category, category_path, title, path, subtitle_path, cover_path, description);
    library::create_item(&db, &it)
}

#[tauri::command]
fn media_delete(db: tauri::State<Db>, id: i64) -> AppResult<()> {
    library::delete_item(&db, id)
}

/// 把 src 图拷到 <app_data>/covers，返回拷贝后绝对路径（供前端填 coverPath）。
#[tauri::command(rename_all = "camelCase")]
fn import_cover(app: tauri::AppHandle, src_image: String) -> AppResult<String> {
    let covers = app
        .path()
        .app_data_dir()
        .map_err(|e| error::AppError::Other(format!("app_data_dir: {e}")))?
        .join("covers");
    library::cover::import_cover(&covers, &src_image)
}

/// 读/写 TMDB API Key（存 settings 表）。
#[tauri::command]
fn set_tmdb_key(db: tauri::State<Db>, key: String) -> AppResult<()> {
    settings::set(&db, "tmdb_api_key", &key)
}

#[tauri::command]
fn get_tmdb_key(db: tauri::State<Db>) -> AppResult<Option<String>> {
    settings::get(&db, "tmdb_api_key")
}

/// 为所有空封面视频抓取 TMDB 海报，后台线程执行，poster-progress 事件推进度。
#[tauri::command]
async fn fetch_posters(app: tauri::AppHandle) -> AppResult<poster::FetchReport> {
    use tauri::{Emitter, Manager};

    let db_key = {
        let db = app.state::<Db>();
        settings::get(&db, "tmdb_api_key")?
    };
    let api_key = db_key
        .filter(|k| !k.trim().is_empty())
        .ok_or_else(|| error::AppError::Invalid("请先填写 TMDB API Key".into()))?;

    let covers_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| error::AppError::Other(format!("app_data_dir: {e}")))?
        .join("covers");

    let app2 = app.clone();
    let report = tauri::async_runtime::spawn_blocking(move || -> AppResult<poster::FetchReport> {
        let db = app2.state::<Db>();
        let items = library::list_items(&db, MediaKind::Video)?;

        let key = api_key.clone();
        let covers = covers_dir.clone();
        let app3 = app2.clone();

        let fetch_cover = |q: &poster::parse::MediaQuery| -> Result<String, String> {
            let hit = poster::tmdb::search(&q.name, q.kind, &key)
                .map_err(|e| format!("网络错误: {e}"))?;
            let hit = match hit {
                Some(h) => h,
                None => return Err("搜索无结果".into()),
            };
            let poster_path = if let (poster::parse::MediaKind::Tv, Some(season)) = (q.kind, q.season) {
                match poster::tmdb::season_poster(hit.id, season, &key) {
                    Ok(Some(p)) => Some(p),
                    _ => hit.poster_path.clone(),
                }
            } else {
                hit.poster_path.clone()
            };
            let poster_path = poster_path.ok_or_else(|| "无海报".to_string())?;
            std::thread::sleep(std::time::Duration::from_millis(250));
            let bytes = poster::tmdb::download(&poster_path).map_err(|e| format!("网络错误: {e}"))?;
            let cover = poster::image_proc::to_cover(&bytes).map_err(|e| format!("图片处理失败: {e}"))?;
            let path = poster::image_proc::save_cover(&covers, &cover).map_err(|e| format!("图片处理失败: {e}"))?;
            Ok(path)
        };

        let progress = |done: usize, total: usize, title: &str| {
            let _ = app3.emit("poster-progress", serde_json::json!({
                "done": done, "total": total, "current_title": title
            }));
        };

        poster::fetch_posters(&db, &items, fetch_cover, progress)
    })
    .await
    .map_err(|e| error::AppError::Other(format!("join: {e}")))??;

    Ok(report)
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
            // 启动本地视频 HTTP server（服务 video_cache，支持 Range 流式播放）
            let cache_dir = dir.join("video_cache");
            std::fs::create_dir_all(&cache_dir).ok();
            let port = player::httpserver::start(cache_dir).expect("start video http server");
            app.manage(player::HttpServerState { port });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            set_root,
            get_root,
            scan_root,
            scan_videos_all,
            init_from_demo,
            list_media,
            comic::comic_pages,
            comic::comic_page,
            comic::comic_cover,
            set_comic_page,
            get_comic_page,
            set_video_pos,
            get_video_pos,
            player::player_open,
            player::player_stop,
            launcher::launch_game,
            normalize::normalize_subtitles,
            normalize::comic_pack::normalize_comic,
            media_update,
            media_create,
            media_delete,
            import_cover,
            set_tmdb_key,
            get_tmdb_key,
            fetch_posters
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
