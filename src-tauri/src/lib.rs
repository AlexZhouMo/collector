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
fn list_media(app: tauri::AppHandle, db: tauri::State<Db>, kind: String) -> AppResult<Vec<MediaItem>> {
    let k = MediaKind::from_kind_str(&kind)?;
    let mut items = library::list_items(&db, k)?;
    if matches!(k, MediaKind::Video) {
        let app_data = app
            .path()
            .app_data_dir()
            .ok()
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_default();
        for it in &mut items {
            let root = settings::get(&db, library::paths::video_root_key(&it.category))?
                .unwrap_or_default();
            it.path = library::paths::video_to_absolute(&it.path, &root);
            if let Some(s) = &it.subtitle_path {
                it.subtitle_path = Some(library::paths::appdata_to_absolute(s, &app_data));
            }
            if let Some(c) = &it.cover_path {
                it.cover_path = Some(library::paths::appdata_to_absolute(c, &app_data));
            }
        }
    }
    Ok(items)
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
    app: tauri::AppHandle,
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
    let app_data = app.path().app_data_dir().ok()
        .map(|p| p.to_string_lossy().into_owned()).unwrap_or_default();
    let root = settings::get(&db, library::paths::video_root_key(&category))?.unwrap_or_default();
    let rel_path = library::paths::video_to_relative(&path, &root);
    let rel_sub = subtitle_path.map(|s| library::paths::appdata_to_relative(&s, &app_data));
    let rel_cover = cover_path.map(|c| library::paths::appdata_to_relative(&c, &app_data));
    let it = build_video_item(category, category_path, title, rel_path, rel_sub, rel_cover, description);
    library::update_item(&db, id, &it)
}

#[tauri::command(rename_all = "camelCase")]
fn media_create(
    app: tauri::AppHandle,
    db: tauri::State<Db>,
    category: String,
    category_path: String,
    title: String,
    path: String,
    subtitle_path: Option<String>,
    cover_path: Option<String>,
    description: Option<String>,
) -> AppResult<i64> {
    let app_data = app.path().app_data_dir().ok()
        .map(|p| p.to_string_lossy().into_owned()).unwrap_or_default();
    let root = settings::get(&db, library::paths::video_root_key(&category))?.unwrap_or_default();
    let rel_path = library::paths::video_to_relative(&path, &root);
    let rel_sub = subtitle_path.map(|s| library::paths::appdata_to_relative(&s, &app_data));
    let rel_cover = cover_path.map(|c| library::paths::appdata_to_relative(&c, &app_data));
    let it = build_video_item(category, category_path, title, rel_path, rel_sub, rel_cover, description);
    library::create_item(&db, &it)
}

#[tauri::command]
fn media_delete(db: tauri::State<Db>, id: i64) -> AppResult<()> {
    library::delete_item(&db, id)
}

/// 把 src 图拷到 <app_data>/covers，返回相对路径 covers/xxx（供前端填 coverPath 存库）。
/// 一次性迁移：把库中现有绝对路径改写为相对（视频去分类根+分类名前缀，
/// 字幕/封面去 app_data 前缀），按分类(电影>动漫>剧集)+category_path 排序，
/// 清库重置 id 从 1 重插。执行前请手动备份数据库（不可逆）。
#[tauri::command]
fn migrate_to_relative(app: tauri::AppHandle, db: tauri::State<Db>) -> AppResult<usize> {
    let app_data = app.path().app_data_dir().ok()
        .map(|p| p.to_string_lossy().into_owned()).unwrap_or_default();
    // 三分类 root
    let root_of = |cat: &str| -> String {
        settings::get(&db, library::paths::video_root_key(cat)).ok().flatten().unwrap_or_default()
    };
    // 读全部 video 行（当前绝对路径 + 含分类名 category_path）
    let rows: Vec<(String, String, String, String, Option<String>, Option<String>, Option<String>)> = {
        let conn = db.0.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT category,category_path,title,path,subtitle_path,cover_path,description
             FROM media_item WHERE kind='video'").map_err(|e| error::AppError::Db(e.to_string()))?;
        let it = stmt.query_map([], |r| Ok((
            r.get::<_,String>(0)?, r.get::<_,String>(1)?, r.get::<_,String>(2)?, r.get::<_,String>(3)?,
            r.get::<_,Option<String>>(4)?, r.get::<_,Option<String>>(5)?, r.get::<_,Option<String>>(6)?,
        ))).map_err(|e| error::AppError::Db(e.to_string()))?;
        let mut v=Vec::new();
        for row in it { v.push(row.map_err(|e| error::AppError::Db(e.to_string()))?); }
        v
    };
    // 转相对
    let mut items: Vec<library::scanner::ScannedItem> = rows.into_iter().map(|(cat,cpath,title,path,sub,cover,desc)| {
        let root = root_of(&cat);
        let rel_path = library::paths::video_to_relative(&path, &root);
        let rel_cpath = library::paths::strip_category(&cpath);
        let rel_sub = sub.map(|s| library::paths::appdata_to_relative(&s, &app_data));
        let rel_cover = cover.map(|c| library::paths::appdata_to_relative(&c, &app_data));
        build_video_item(cat, rel_cpath, title, rel_path, rel_sub, rel_cover, desc)
    }).collect();
    // 排序：分类固定序(电影>动漫>剧集) → category_path
    let cat_rank = |c: &str| match c { "电影"=>0, "动漫"=>1, "剧集"=>2, _=>3 };
    items.sort_by(|a,b| cat_rank(&a.category).cmp(&cat_rank(&b.category))
        .then_with(|| a.category_path.cmp(&b.category_path))
        .then_with(|| a.title.cmp(&b.title)));
    // 清库重置重插（事务）
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs() as i64;
    let mut conn = db.0.lock().unwrap();
    let tx = conn.transaction().map_err(|e| error::AppError::Db(e.to_string()))?;
    tx.execute("DELETE FROM media_item WHERE kind='video'", []).map_err(|e| error::AppError::Db(e.to_string()))?;
    tx.execute("DELETE FROM sqlite_sequence WHERE name='media_item'", []).map_err(|e| error::AppError::Db(e.to_string()))?;
    let mut n=0;
    for it in &items {
        tx.execute(
            "INSERT INTO media_item (kind,category,category_path,title,path,subtitle_path,cover_path,description,platform_ok,exec_path,scanned_at)
             VALUES ('video',?1,?2,?3,?4,?5,?6,?7,1,NULL,?8)",
            rusqlite::params![it.category, it.category_path, it.title, it.path, it.subtitle_path, it.cover_path, it.description, now],
        ).map_err(|e| error::AppError::Db(e.to_string()))?;
        n+=1;
    }
    tx.commit().map_err(|e| error::AppError::Db(e.to_string()))?;
    Ok(n)
}

#[tauri::command(rename_all = "camelCase")]
fn import_cover(app: tauri::AppHandle, src_image: String) -> AppResult<String> {
    let app_data = app
        .path()
        .app_data_dir()
        .map_err(|e| error::AppError::Other(format!("app_data_dir: {e}")))?;
    let covers = app_data.join("covers");
    let abs = library::cover::import_cover(&covers, &src_image)?;
    Ok(library::paths::appdata_to_relative(&abs, &app_data.to_string_lossy()))
}

/// 按裁剪矩形 (x,y,w,h) 从原图生成标准海报（500×750 JPEG q85，与自动抓取一致），
/// 存入 <app_data>/covers，返回相对路径 covers/xxx（磁盘仍写绝对位置）。
#[tauri::command(rename_all = "camelCase")]
fn import_cover_cropped(
    app: tauri::AppHandle,
    src_image: String,
    x: u32,
    y: u32,
    w: u32,
    h: u32,
) -> AppResult<String> {
    let app_data = app
        .path()
        .app_data_dir()
        .map_err(|e| error::AppError::Other(format!("app_data_dir: {e}")))?;
    let covers = app_data.join("covers");
    let bytes = std::fs::read(&src_image)
        .map_err(|e| error::AppError::Other(format!("read cover source: {e}")))?;
    let cover = poster::image_proc::crop_to_cover(&bytes, x, y, w, h)?;
    let abs = poster::image_proc::save_cover(&covers, &cover)?;
    Ok(library::paths::appdata_to_relative(&abs, &app_data.to_string_lossy()))
}

/// 删除封面文件。仅允许删 <app_data>/covers/ 目录内的文件（防路径穿越）；
/// 目录外或不存在的路径静默忽略，不误删、不报错。
#[tauri::command(rename_all = "camelCase")]
fn delete_cover_file(app: tauri::AppHandle, path: String) -> AppResult<()> {
    let covers = app
        .path()
        .app_data_dir()
        .map_err(|e| error::AppError::Other(format!("app_data_dir: {e}")))?
        .join("covers");
    let canon_covers = covers.canonicalize().unwrap_or(covers);
    if let Ok(ct) = std::path::Path::new(&path).canonicalize() {
        if ct.starts_with(&canon_covers) {
            std::fs::remove_file(&ct).ok(); // 不存在忽略
        }
    }
    Ok(())
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
        let app_data = covers_dir
            .parent()
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_default();
        let app3 = app2.clone();

        let fetch_cover = |q: &poster::parse::MediaQuery| -> Result<String, String> {
            let mut hit = poster::tmdb::search(&q.name, q.kind, q.year, &key)
                .map_err(|e| format!("网络错误: {e}"))?;
            // 动漫：先按 movie（剧场版）搜，未命中 fallback 搜 tv（TV 动画）
            if hit.is_none() && q.is_anime {
                hit = poster::tmdb::search(&q.name, poster::parse::MediaKind::Tv, q.year, &key)
                    .map_err(|e| format!("网络错误: {e}"))?;
            }
            // 副标题降级：完整名未命中时，用冒号前主名再搜一遍（含动漫 fallback）
            if hit.is_none() {
                if let Some(alt) = &q.alt_name {
                    hit = poster::tmdb::search(alt, q.kind, q.year, &key)
                        .map_err(|e| format!("网络错误: {e}"))?;
                    if hit.is_none() && q.is_anime {
                        hit = poster::tmdb::search(alt, poster::parse::MediaKind::Tv, q.year, &key)
                            .map_err(|e| format!("网络错误: {e}"))?;
                    }
                    // 降级命中年份校验：条目有年份且命中年份存在时须 ±1，否则视为未命中，
                    // 防止主名搜到同系列错年份的片（如 非常人贩：重启之战 误配 2002 初代）
                    if let Some(h) = &hit {
                        if let (Some(y), Some(hy)) = (q.year, h.year) {
                            if (y as i64 - hy as i64).abs() > 1 {
                                hit = None;
                            }
                        }
                    }
                }
            }
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
            Ok(library::paths::appdata_to_relative(&path, &app_data))
        };

        let progress = |done: usize, total: usize, title: &str| {
            let _ = app3.emit("poster-progress", serde_json::json!({
                "done": done, "total": total, "current_title": title
            }));
        };

        // 优化建议：对失败组用多标点分词生成候选，逐个联 TMDB 探测，年份校验后
        // 取最匹配的单个候选名（候选按长度降序，最长=最接近完整片名）。不改库，仅供参考。
        let suggest = |q: &poster::parse::MediaQuery, reason: &str| -> (Option<String>, String) {
            // 剧集季无海报：回退整剧，属可接受，不建议改名
            if q.kind == poster::parse::MediaKind::Tv && q.season.is_some() && reason == "无海报" {
                return (None, "该季 TMDB 无独立海报，将回退整剧海报（可接受）".to_string());
            }
            // 多标点分词生成候选（越长越靠前）
            let cands = poster::parse::suggest_candidates(&q.name);
            for cand in &cands {
                if cand == &q.name {
                    continue; // 完整原名已在主流程搜过，跳过
                }
                let kinds = if q.is_anime {
                    vec![poster::parse::MediaKind::Movie, poster::parse::MediaKind::Tv]
                } else {
                    vec![q.kind]
                };
                for k in kinds {
                    if let Ok(Some(hit)) = poster::tmdb::search_detailed(cand, k, q.year, &key) {
                        if let (Some(y), Some(hy)) = (q.year, hit.year) {
                            if (y as i64 - hy as i64).abs() <= 1 && !hit.title.is_empty() {
                                // 取第一个通过年份校验的命中（最长候选优先）作为最匹配建议
                                return (Some(hit.title), "译名/名称与 TMDB 不符，改为此名可命中".to_string());
                            }
                        }
                    }
                }
            }
            (None, "未找到可靠候选，请手动查证官方译名，或用编辑封面手动上传".to_string())
        };

        poster::fetch_posters(&db, &items, fetch_cover, suggest, progress)
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
            std::fs::create_dir_all(dir.join("covers")).ok();
            std::fs::create_dir_all(dir.join("subtitles")).ok();
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
            migrate_to_relative,
            import_cover,
            import_cover_cropped,
            delete_cover_file,
            set_tmdb_key,
            get_tmdb_key,
            fetch_posters
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
