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
            if let Some(s) = &it.subtitle_path {
                it.subtitle_path = Some(library::paths::appdata_to_absolute(s, &app_data));
            }
            if let Some(c) = &it.cover_path {
                it.cover_path = Some(library::paths::appdata_to_absolute(c, &app_data));
            }
            let root = settings::get(&db, library::paths::video_root_key(&it.category))?
                .unwrap_or_default();
            let abs = library::paths::video_abs_path(&root, &it.category_path, &it.title);
            it.playable = !abs.is_empty() && std::path::Path::new(&abs).is_file();
            it.video_path = abs;
        }
    } else {
        for it in &mut items {
            it.playable = true;
        }
    }
    Ok(items)
}

/// 用前端传的字段构造一个 video ScannedItem。
/// 视频文件路径不再入库，运行时由 root+category_path+title 拼接推导。
fn build_video_item(
    category: String,
    category_path: String,
    title: String,
    subtitle_path: Option<String>,
    cover_path: Option<String>,
    description: Option<String>,
) -> library::scanner::ScannedItem {
    library::scanner::ScannedItem {
        category,
        category_path,
        title,
        subtitle_path,
        cover_path,
        description,
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
    subtitle_path: Option<String>,
    cover_path: Option<String>,
    description: Option<String>,
) -> AppResult<()> {
    let app_data = app.path().app_data_dir().ok()
        .map(|p| p.to_string_lossy().into_owned()).unwrap_or_default();
    let subtitles_dir = std::path::Path::new(&app_data).join("subtitles");
    let rel_sub = subtitle_path.map(|s| {
        // 已是相对(subtitles/开头)则原样；否则拷进 app_data/subtitles 再转相对
        if s.starts_with("subtitles/") { return s; }
        match library::subtitle::import_subtitle(&subtitles_dir, &s) {
            Ok(abs) => library::paths::appdata_to_relative(&abs, &app_data),
            Err(_) => library::paths::appdata_to_relative(&s, &app_data), // 拷贝失败退回原逻辑
        }
    });
    let rel_cover = cover_path.map(|c| library::paths::appdata_to_relative(&c, &app_data));
    let it = build_video_item(category, category_path, title, rel_sub, rel_cover, description);
    library::update_item(&db, id, &it)
}

/// 由旧相对路径与新末段名算出新相对路径：保留父前缀，替换最后一段。
fn rename_target_path(old_path: &str, new_name: &str) -> String {
    match old_path.rfind('/') {
        Some(i) => format!("{}/{}", &old_path[..i], new_name),
        None => new_name.to_string(),
    }
}

/// 校验 + 事务级联更新 category_path（不含磁盘操作，便于单测）。
fn rename_folder_in_db(db: &Db, category: &str, old_path: &str, new_name: &str) -> AppResult<()> {
    let name = new_name.trim();
    if name.is_empty() || name.contains('/') {
        return Err(crate::error::AppError::Other("名称无效".into()));
    }
    let new_path = rename_target_path(old_path, name);
    if new_path == old_path {
        return Ok(());
    }
    let conn = db.0.lock().unwrap();
    let exists: i64 = conn.query_row(
        "SELECT COUNT(*) FROM media WHERE category=?1 AND (category_path=?2 OR category_path LIKE ?2 || '/%')",
        rusqlite::params![category, new_path],
        |r| r.get(0),
    ).map_err(|e| crate::error::AppError::Db(e.to_string()))?;
    if exists > 0 {
        return Err(crate::error::AppError::Other("已存在同名文件夹".into()));
    }
    conn.execute(
        "UPDATE media SET category_path = ?1 || substr(category_path, length(?2)+1) \
         WHERE category=?3 AND category_path LIKE ?2 || '/%'",
        rusqlite::params![new_path, old_path, category],
    ).map_err(|e| crate::error::AppError::Db(e.to_string()))?;
    conn.execute(
        "UPDATE media SET category_path = ?1 WHERE category=?2 AND category_path = ?3",
        rusqlite::params![new_path, category, old_path],
    ).map_err(|e| crate::error::AppError::Db(e.to_string()))?;
    Ok(())
}

/// 重命名分类内某文件夹：先 rename 磁盘视频目录，再级联更新库 category_path。
#[tauri::command(rename_all = "camelCase")]
fn rename_folder(
    db: tauri::State<Db>,
    category: String,
    old_path: String,
    new_name: String,
) -> AppResult<()> {
    let name = new_name.trim();
    if name.is_empty() || name.contains('/') {
        return Err(crate::error::AppError::Other("名称无效".into()));
    }
    let new_path = rename_target_path(&old_path, name);
    if new_path == old_path {
        return Ok(());
    }
    let root = crate::settings::get(&db, crate::library::paths::video_root_key(&category))?
        .unwrap_or_default();
    if !root.is_empty() {
        let src = std::path::Path::new(&root).join(&old_path);
        let dst = std::path::Path::new(&root).join(&new_path);
        if src.is_dir() {
            std::fs::rename(&src, &dst)
                .map_err(|e| crate::error::AppError::Other(format!("重命名文件夹失败: {e}")))?;
        }
    }
    rename_folder_in_db(&db, &category, &old_path, name)
}

#[tauri::command(rename_all = "camelCase")]
fn media_create(
    app: tauri::AppHandle,
    db: tauri::State<Db>,
    category: String,
    category_path: String,
    title: String,
    subtitle_path: Option<String>,
    cover_path: Option<String>,
    description: Option<String>,
) -> AppResult<i64> {
    let app_data = app.path().app_data_dir().ok()
        .map(|p| p.to_string_lossy().into_owned()).unwrap_or_default();
    let subtitles_dir = std::path::Path::new(&app_data).join("subtitles");
    let rel_sub = subtitle_path.map(|s| {
        // 已是相对(subtitles/开头)则原样；否则拷进 app_data/subtitles 再转相对
        if s.starts_with("subtitles/") { return s; }
        match library::subtitle::import_subtitle(&subtitles_dir, &s) {
            Ok(abs) => library::paths::appdata_to_relative(&abs, &app_data),
            Err(_) => library::paths::appdata_to_relative(&s, &app_data), // 拷贝失败退回原逻辑
        }
    });
    let rel_cover = cover_path.map(|c| library::paths::appdata_to_relative(&c, &app_data));
    let it = build_video_item(category, category_path, title, rel_sub, rel_cover, description);
    library::create_item(&db, MediaKind::Video, &it)
}

#[tauri::command]
fn media_delete(db: tauri::State<Db>, id: i64) -> AppResult<()> {
    library::delete_item(&db, id)
}

/// 把 src 图拷到 <app_data>/covers，返回相对路径 covers/xxx（供前端填 coverPath 存库）。

#[tauri::command(rename_all = "camelCase")]
fn import_cover(app: tauri::AppHandle, src_image: String) -> AppResult<String> {
    let app_data = app
        .path()
        .app_data_dir()
        .map_err(|e| error::AppError::Other(format!("app_data_dir: {e}")))?;
    let covers = app_data.join("covers");
    let abs = library::cover::import_cover(&covers, &src_image)?;
    // 返回绝对路径供前端预览；保存时 media_update 会转相对存库。
    Ok(abs)
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
    let abs = poster::image_proc::save_cover(&covers, &cover, "cover_")?;
    // 返回绝对路径供前端 convertFileSrc 预览显示；保存时 media_update 会转相对存库。
    Ok(abs)
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
            let path = poster::image_proc::save_cover(&covers, &cover, "tmdb_").map_err(|e| format!("图片处理失败: {e}"))?;
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
            player::player_open,
            player::player_stop,
            launcher::launch_game,
            normalize::normalize_subtitles,
            normalize::comic_pack::normalize_comic,
            media_update,
            rename_folder,
            media_create,
            media_delete,
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

#[cfg(test)]
mod rename_folder_tests {
    use super::*;
    use crate::db::Db;

    fn seed(db: &Db, cat: &str, cpath: &str) {
        let conn = db.0.lock().unwrap();
        conn.execute(
            "INSERT INTO media (category,category_path,title) VALUES (?1,?2,?3)",
            rusqlite::params![cat, cpath, format!("t_{cpath}")],
        ).unwrap();
    }
    fn cpath_of(db: &Db, title: &str) -> String {
        let conn = db.0.lock().unwrap();
        conn.query_row("SELECT category_path FROM media WHERE title=?1",
            rusqlite::params![title], |r| r.get(0)).unwrap()
    }

    #[test]
    fn target_path_root_and_nested() {
        assert_eq!(rename_target_path("科幻", "科幻片"), "科幻片");
        assert_eq!(rename_target_path("科幻/系列", "系列2"), "科幻/系列2");
        assert_eq!(rename_target_path("a/b/c", "x"), "a/b/x");
    }

    #[test]
    fn cascade_updates_self_and_descendants_only() {
        let db = Db::open_in_memory().unwrap();
        seed(&db, "电影", "科幻");
        seed(&db, "电影", "科幻/星战");
        seed(&db, "电影", "科幻小说");
        seed(&db, "电影", "奇幻");
        rename_folder_in_db(&db, "电影", "科幻", "科幻片").unwrap();
        assert_eq!(cpath_of(&db, "t_科幻"), "科幻片");
        assert_eq!(cpath_of(&db, "t_科幻/星战"), "科幻片/星战");
        assert_eq!(cpath_of(&db, "t_科幻小说"), "科幻小说");
        assert_eq!(cpath_of(&db, "t_奇幻"), "奇幻");
    }

    #[test]
    fn reject_sibling_name_conflict() {
        let db = Db::open_in_memory().unwrap();
        seed(&db, "电影", "科幻");
        seed(&db, "电影", "奇幻");
        assert!(rename_folder_in_db(&db, "电影", "科幻", "奇幻").is_err());
    }

    #[test]
    fn reject_empty_or_slash_name() {
        let db = Db::open_in_memory().unwrap();
        seed(&db, "电影", "科幻");
        assert!(rename_folder_in_db(&db, "电影", "科幻", "").is_err());
        assert!(rename_folder_in_db(&db, "电影", "科幻", "  ").is_err());
        assert!(rename_folder_in_db(&db, "电影", "科幻", "a/b").is_err());
    }
}
