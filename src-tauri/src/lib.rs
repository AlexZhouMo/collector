mod comic;
mod db;
mod error;
mod launcher;
mod library;
mod maintenance;
mod migrate;
mod normalize;
mod player;
mod poster;
mod settings;
mod util;

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
fn list_media(app: tauri::AppHandle, db: tauri::State<Db>, kind: String) -> AppResult<Vec<MediaItem>> {
    let k = MediaKind::from_kind_str(&kind)?;
    let mut items = library::list_items(&db, k)?;
    let app_data = app
        .path()
        .app_data_dir()
        .ok()
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_default();
    // 封面统一转绝对路径（DB 存相对 covers/...），前端 convertFileSrc 才能加载。
    for it in &mut items {
        if let Some(c) = &it.cover_path {
            it.cover_path = Some(library::paths::appdata_to_absolute(c, &app_data));
        }
    }
    if matches!(k, MediaKind::Video) {
        for it in &mut items {
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
    cover_path: Option<String>,
    description: Option<String>,
) -> library::scanner::ScannedItem {
    library::scanner::ScannedItem {
        category,
        category_path,
        title,
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
    cover_path: Option<String>,
    description: Option<String>,
) -> AppResult<()> {
    let app_data = app.path().app_data_dir().ok()
        .map(|p| p.to_string_lossy().into_owned()).unwrap_or_default();
    // 读旧值（锁在块内释放，避免与 update_item 内部 lock 死锁）
    let old_meta: Option<(String, String, String)> = {
        let conn = db.0.lock().unwrap();
        conn.query_row(
            "SELECT category,category_path,title FROM media WHERE id=?1",
            rusqlite::params![id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        ).ok()
    };
    // 新推导目标（在 category/category_path/title 被 move 前算好）
    let new_sub = library::paths::subtitle_abs_path(&app_data, &category, &category_path, &title);
    let rel_cover = cover_path.map(|c| library::paths::appdata_to_relative(&c, &app_data));
    let it = build_video_item(category, category_path, title, rel_cover, description);
    library::update_item(&db, id, &it)?;
    // 字幕单条跟随：旧推导路径 → 新推导路径（容错跳过）
    if let Some((oc, op, ot)) = old_meta {
        let src = library::paths::subtitle_abs_path(&app_data, &oc, &op, &ot);
        if src != new_sub && std::path::Path::new(&src).is_file() {
            if let Some(p) = std::path::Path::new(&new_sub).parent() {
                let _ = std::fs::create_dir_all(p);
            }
            let _ = std::fs::rename(&src, &new_sub);
        }
    }
    Ok(())
}

/// 由旧相对路径与新末段名算出新相对路径：保留父前缀，替换最后一段。
fn rename_target_path(old_path: &str, new_name: &str) -> String {
    match old_path.rfind('/') {
        Some(i) => format!("{}/{}", &old_path[..i], new_name),
        None => new_name.to_string(),
    }
}

/// 校验 + 事务级联更新 category_path（不含磁盘操作，便于单测）。
/// video（media 表，有 category 列）按 category=? 过滤级联；comic/game（无 category 列）
/// 仅按 category_path 前缀级联。表名由 kind 决定，来自枚举（非用户输入），拼接无注入风险。
fn rename_folder_in_db(db: &Db, kind: MediaKind, category: &str, old_path: &str, new_name: &str) -> AppResult<()> {
    let name = new_name.trim();
    if name.is_empty() || name.contains('/') {
        return Err(crate::error::AppError::Other("名称无效".into()));
    }
    let new_path = rename_target_path(old_path, name);
    if new_path == old_path {
        return Ok(());
    }
    let table = kind.table_name();
    let has_category = matches!(kind, MediaKind::Video);
    let conn = db.0.lock().unwrap();
    if has_category {
        let exists: i64 = conn.query_row(
            &format!("SELECT COUNT(*) FROM {table} WHERE category=?1 AND (category_path=?2 OR category_path LIKE ?2 || '/%')"),
            rusqlite::params![category, new_path],
            |r| r.get(0),
        ).map_err(|e| crate::error::AppError::Db(e.to_string()))?;
        if exists > 0 {
            return Err(crate::error::AppError::Other("已存在同名文件夹".into()));
        }
        conn.execute(
            &format!("UPDATE {table} SET category_path = ?1 || substr(category_path, length(?2)+1) \
             WHERE category=?3 AND category_path LIKE ?2 || '/%'"),
            rusqlite::params![new_path, old_path, category],
        ).map_err(|e| crate::error::AppError::Db(e.to_string()))?;
        conn.execute(
            &format!("UPDATE {table} SET category_path = ?1 WHERE category=?2 AND category_path = ?3"),
            rusqlite::params![new_path, category, old_path],
        ).map_err(|e| crate::error::AppError::Db(e.to_string()))?;
    } else {
        let exists: i64 = conn.query_row(
            &format!("SELECT COUNT(*) FROM {table} WHERE category_path=?1 OR category_path LIKE ?1 || '/%'"),
            rusqlite::params![new_path],
            |r| r.get(0),
        ).map_err(|e| crate::error::AppError::Db(e.to_string()))?;
        if exists > 0 {
            return Err(crate::error::AppError::Other("已存在同名文件夹".into()));
        }
        conn.execute(
            &format!("UPDATE {table} SET category_path = ?1 || substr(category_path, length(?2)+1) \
             WHERE category_path LIKE ?2 || '/%'"),
            rusqlite::params![new_path, old_path],
        ).map_err(|e| crate::error::AppError::Db(e.to_string()))?;
        conn.execute(
            &format!("UPDATE {table} SET category_path = ?1 WHERE category_path = ?2"),
            rusqlite::params![new_path, old_path],
        ).map_err(|e| crate::error::AppError::Db(e.to_string()))?;
    }
    Ok(())
}

/// 重命名分类内某文件夹：video 先 rename 磁盘目录 + 字幕跟随，再级联更新库 category_path；
/// comic/game 仅改库（DB-only，不动磁盘）。kind 由前端传（video/comic/game）。
#[tauri::command(rename_all = "camelCase")]
fn rename_folder(
    app: tauri::AppHandle,
    db: tauri::State<Db>,
    kind: String,
    category: String,
    old_path: String,
    new_name: String,
) -> AppResult<()> {
    let k = MediaKind::from_kind_str(&kind)?;
    let name = new_name.trim();
    if name.is_empty() || name.contains('/') {
        return Err(crate::error::AppError::Other("名称无效".into()));
    }
    let new_path = rename_target_path(&old_path, name);
    if new_path == old_path {
        return Ok(());
    }
    // 仅 video 需要磁盘 rename 与字幕跟随；comic/game 为 DB-only。
    if matches!(k, MediaKind::Video) {
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
        // 字幕目录跟随：<app_data>/subtitles/<category>/<old_path> → <new_path>（容错跳过）
        if let Ok(app_data) = app.path().app_data_dir() {
            let base = app_data.join("subtitles").join(&category);
            let s_src = base.join(&old_path);
            let s_dst = base.join(&new_path);
            if s_src.is_dir() {
                if let Some(p) = s_dst.parent() {
                    let _ = std::fs::create_dir_all(p);
                }
                let _ = std::fs::rename(&s_src, &s_dst);
            }
        }
    }
    rename_folder_in_db(&db, k, &category, &old_path, name)
}

/// 由旧相对路径算末段文件夹名（old_path 可能无 "/"）。
fn folder_leaf(old_path: &str) -> &str {
    match old_path.rfind('/') {
        Some(i) => &old_path[i + 1..],
        None => old_path,
    }
}

/// 由 target_parent + folder_name 算移动后的新相对路径。
fn move_target_path(target_parent: &str, folder_name: &str) -> String {
    if target_parent.is_empty() {
        folder_name.to_string()
    } else {
        format!("{target_parent}/{folder_name}")
    }
}

/// 移动文件夹：把 (category, old_path) 整棵子树移到 (target_category, target_parent) 下。
/// new_path = target_parent 空 ? folder_name : target_parent/folder_name（folder_name=old_path 末段）。
/// video 同时改 category=target_category；comic/game 的 target_category 必须==category。
/// 校验：① target 属同 kind（调用方保证）；② 防移进自身：同分类且 new_path==old_path 或
/// new_path 以 old_path+"/" 开头 → Err；③ 目标 (target_category,new_path) 及其位置已存在同名 → Err。
/// 表名由 kind 决定（枚举，非注入）。
fn move_folder_in_db(
    db: &Db,
    kind: MediaKind,
    category: &str,
    old_path: &str,
    target_category: &str,
    target_parent: &str,
) -> AppResult<()> {
    let has_category = matches!(kind, MediaKind::Video);
    // comic/game 单分类，target_category 必须与 category 一致。
    if !has_category && target_category != category {
        return Err(crate::error::AppError::Other("该类型不支持跨分类移动".into()));
    }
    let folder_name = folder_leaf(old_path);
    if folder_name.is_empty() {
        return Err(crate::error::AppError::Other("源路径无效".into()));
    }
    let new_path = move_target_path(target_parent, folder_name);
    // 防移进自身或子目录（仅同分类时才可能）。
    if target_category == category
        && (new_path == old_path || new_path.starts_with(&format!("{old_path}/")))
    {
        return Err(crate::error::AppError::Other("不能移动到自身或子目录".into()));
    }
    let table = kind.table_name();
    let conn = db.0.lock().unwrap();
    if has_category {
        let exists: i64 = conn.query_row(
            &format!("SELECT COUNT(*) FROM {table} WHERE category=?1 AND (category_path=?2 OR category_path LIKE ?2 || '/%')"),
            rusqlite::params![target_category, new_path],
            |r| r.get(0),
        ).map_err(|e| crate::error::AppError::Db(e.to_string()))?;
        if exists > 0 {
            return Err(crate::error::AppError::Other("目标已存在同名文件夹".into()));
        }
        // 子孙：前缀替换 + 改 category。
        conn.execute(
            &format!("UPDATE {table} SET category=?1, category_path = ?2 || substr(category_path, length(?3)+1) \
             WHERE category=?4 AND category_path LIKE ?3 || '/%'"),
            rusqlite::params![target_category, new_path, old_path, category],
        ).map_err(|e| crate::error::AppError::Db(e.to_string()))?;
        // self：精确匹配 + 改 category。
        conn.execute(
            &format!("UPDATE {table} SET category=?1, category_path = ?2 WHERE category=?3 AND category_path = ?4"),
            rusqlite::params![target_category, new_path, category, old_path],
        ).map_err(|e| crate::error::AppError::Db(e.to_string()))?;
    } else {
        let exists: i64 = conn.query_row(
            &format!("SELECT COUNT(*) FROM {table} WHERE category_path=?1 OR category_path LIKE ?1 || '/%'"),
            rusqlite::params![new_path],
            |r| r.get(0),
        ).map_err(|e| crate::error::AppError::Db(e.to_string()))?;
        if exists > 0 {
            return Err(crate::error::AppError::Other("目标已存在同名文件夹".into()));
        }
        conn.execute(
            &format!("UPDATE {table} SET category_path = ?1 || substr(category_path, length(?2)+1) \
             WHERE category_path LIKE ?2 || '/%'"),
            rusqlite::params![new_path, old_path],
        ).map_err(|e| crate::error::AppError::Db(e.to_string()))?;
        conn.execute(
            &format!("UPDATE {table} SET category_path = ?1 WHERE category_path = ?2"),
            rusqlite::params![new_path, old_path],
        ).map_err(|e| crate::error::AppError::Db(e.to_string()))?;
    }
    Ok(())
}

/// 移动文件夹（连同子孙）到同类型下另一位置：video 字幕目录跟随（先字幕后 DB，与 rename_folder 一致）；
/// comic/game DB-only（单分类，target_category 必须==category）。kind 由前端传（video/comic/game）。
#[tauri::command(rename_all = "camelCase")]
fn move_folder(
    app: tauri::AppHandle,
    db: tauri::State<Db>,
    kind: String,
    category: String,
    old_path: String,
    target_category: String,
    target_parent: String,
) -> AppResult<()> {
    let k = MediaKind::from_kind_str(&kind)?;
    let folder_name = folder_leaf(&old_path);
    if folder_name.is_empty() {
        return Err(crate::error::AppError::Other("源路径无效".into()));
    }
    let new_path = move_target_path(&target_parent, folder_name);
    // 仅 video 需要字幕目录跟随；comic/game 为 DB-only。
    if matches!(k, MediaKind::Video) {
        // 字幕：<app_data>/subtitles/<category>/<old_path> → <app_data>/subtitles/<target_category>/<new_path>
        if let Ok(app_data) = app.path().app_data_dir() {
            let s_src = app_data.join("subtitles").join(&category).join(&old_path);
            let s_dst = app_data
                .join("subtitles")
                .join(&target_category)
                .join(&new_path);
            if s_src.is_dir() {
                if let Some(p) = s_dst.parent() {
                    let _ = std::fs::create_dir_all(p);
                }
                let _ = std::fs::rename(&s_src, &s_dst);
            }
        }
    }
    move_folder_in_db(&db, k, &category, &old_path, &target_category, &target_parent)
}

#[tauri::command(rename_all = "camelCase")]
fn media_create(
    app: tauri::AppHandle,
    db: tauri::State<Db>,
    kind: String,
    category: String,
    category_path: String,
    title: String,
    cover_path: Option<String>,
    description: Option<String>,
) -> AppResult<i64> {
    let media_kind = MediaKind::from_kind_str(&kind)?;
    let app_data = app.path().app_data_dir().ok()
        .map(|p| p.to_string_lossy().into_owned()).unwrap_or_default();
    let rel_cover = cover_path.map(|c| library::paths::appdata_to_relative(&c, &app_data));
    let it = build_video_item(category, category_path, title, rel_cover, description);
    library::create_item(&db, media_kind, &it)
}

#[tauri::command]
fn media_delete(db: tauri::State<Db>, id: i64) -> AppResult<()> {
    library::delete_item(&db, id)
}

/// 更新 comic/game 表单条记录（表名受控，非用户输入）。
fn update_media_kind_row(db: &Db, table: &str, id: i64, category_path: &str, title: &str, cover_path: Option<&str>, description: Option<&str>) -> AppResult<()> {
    let conn = db.0.lock().unwrap();
    conn.execute(
        &format!("UPDATE {table} SET category_path=?1,title=?2,cover_path=?3,description=?4 WHERE id=?5"),
        rusqlite::params![category_path, title, cover_path, description, id],
    ).map_err(|e| error::AppError::Db(e.to_string()))?;
    Ok(())
}

/// 删除 comic/game 表单条记录（表名受控，非用户输入）。
fn delete_media_kind_row(db: &Db, table: &str, id: i64) -> AppResult<()> {
    db.0.lock().unwrap()
        .execute(&format!("DELETE FROM {table} WHERE id=?1"), rusqlite::params![id])
        .map_err(|e| error::AppError::Db(e.to_string()))?;
    Ok(())
}

#[tauri::command(rename_all = "camelCase")]
fn comic_update(app: tauri::AppHandle, db: tauri::State<Db>, id: i64, category_path: String, title: String, cover_path: Option<String>, description: Option<String>) -> AppResult<()> {
    let app_data = app.path().app_data_dir().ok().map(|p| p.to_string_lossy().into_owned()).unwrap_or_default();
    let rel_cover = cover_path.map(|c| library::paths::appdata_to_relative(&c, &app_data));
    update_media_kind_row(&db, "comic", id, &category_path, &title, rel_cover.as_deref(), description.as_deref())
}

#[tauri::command]
fn comic_delete(db: tauri::State<Db>, id: i64) -> AppResult<()> {
    delete_media_kind_row(&db, "comic", id)
}

#[tauri::command(rename_all = "camelCase")]
fn game_update(app: tauri::AppHandle, db: tauri::State<Db>, id: i64, category_path: String, title: String, cover_path: Option<String>, description: Option<String>) -> AppResult<()> {
    let app_data = app.path().app_data_dir().ok().map(|p| p.to_string_lossy().into_owned()).unwrap_or_default();
    let rel_cover = cover_path.map(|c| library::paths::appdata_to_relative(&c, &app_data));
    update_media_kind_row(&db, "game", id, &category_path, &title, rel_cover.as_deref(), description.as_deref())
}

#[tauri::command]
fn game_delete(db: tauri::State<Db>, id: i64) -> AppResult<()> {
    delete_media_kind_row(&db, "game", id)
}

/// kind → 封面存放子目录（comic/game 各自子目录，其余归 media）。
fn cover_subdir(kind: &str) -> &'static str {
    match kind {
        "comic" => "comic",
        "game" => "game",
        _ => "media",
    }
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
    kind: String,
) -> AppResult<String> {
    let app_data = app
        .path()
        .app_data_dir()
        .map_err(|e| error::AppError::Other(format!("app_data_dir: {e}")))?;
    let sub = cover_subdir(&kind);
    let covers = app_data.join("covers").join(sub);
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

#[tauri::command(rename_all = "camelCase")]
fn get_subtitle_input_dir(db: tauri::State<Db>) -> AppResult<Option<String>> {
    settings::get(&db, "subtitle_input_dir")
}

#[tauri::command(rename_all = "camelCase")]
fn set_subtitle_input_dir(db: tauri::State<Db>, path: String) -> AppResult<()> {
    settings::set(&db, "subtitle_input_dir", &path)
}

/// 单组封面抓取：按 MediaQuery 联 TMDB 搜索、下载、处理并保存，返回相对封面路径。
/// 从 fetch_posters 内联闭包提取，捕获变量 key/covers/app_data 改为显式参数。
fn fetch_cover(
    q: &poster::parse::MediaQuery,
    key: &str,
    covers: &std::path::Path,
    app_data: &str,
) -> Result<String, String> {
    let mut hit = poster::tmdb::search(&q.name, q.kind, q.year, key)
        .map_err(|e| format!("网络错误: {e}"))?;
    // 动漫：先按 movie（剧场版）搜，未命中 fallback 搜 tv（TV 动画）
    if hit.is_none() && q.is_anime {
        hit = poster::tmdb::search(&q.name, poster::parse::MediaKind::Tv, q.year, key)
            .map_err(|e| format!("网络错误: {e}"))?;
    }
    // 副标题降级：完整名未命中时，用冒号前主名再搜一遍（含动漫 fallback）
    if hit.is_none() {
        if let Some(alt) = &q.alt_name {
            hit = poster::tmdb::search(alt, q.kind, q.year, key)
                .map_err(|e| format!("网络错误: {e}"))?;
            if hit.is_none() && q.is_anime {
                hit = poster::tmdb::search(alt, poster::parse::MediaKind::Tv, q.year, key)
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
        match poster::tmdb::season_poster(hit.id, season, key) {
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
    let path = poster::image_proc::save_cover(covers, &cover, "tmdb_").map_err(|e| format!("图片处理失败: {e}"))?;
    Ok(library::paths::appdata_to_relative(&path, app_data))
}

/// 失败组改名建议：对失败组用多标点分词生成候选，逐个联 TMDB 探测，年份校验后
/// 取最匹配的单个候选名（候选按长度降序，最长=最接近完整片名）。不改库，仅供参考。
/// 从 fetch_posters 内联闭包提取，捕获变量 key 改为显式参数。
fn suggest(
    q: &poster::parse::MediaQuery,
    reason: &str,
    key: &str,
) -> (Option<String>, String) {
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
            if let Ok(Some(hit)) = poster::tmdb::search_detailed(cand, k, q.year, key) {
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
        .join("covers")
        .join("media");

    let app2 = app.clone();
    let report = tauri::async_runtime::spawn_blocking(move || -> AppResult<poster::FetchReport> {
        let db = app2.state::<Db>();
        let items = library::list_items(&db, MediaKind::Video)?;

        let key = api_key.clone();
        let covers = covers_dir.clone();
        // covers_dir 现为 <app_data>/covers/media，app_data 需回退两层
        let app_data = covers_dir
            .parent()
            .and_then(|p| p.parent())
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_default();
        let app3 = app2.clone();

        let progress = |done: usize, total: usize, title: &str| {
            let _ = app3.emit("poster-progress", serde_json::json!({
                "done": done, "total": total, "current_title": title
            }));
        };

        // 优化建议：对失败组用多标点分词生成候选，逐个联 TMDB 探测，年份校验后
        // 取最匹配的单个候选名（候选按长度降序，最长=最接近完整片名）。不改库，仅供参考。
        // fetch_cover/suggest 已提为具名函数，此处用闭包补上原捕获的 key/covers/app_data。
        let fetch_cover = |q: &poster::parse::MediaQuery| fetch_cover(q, &key, &covers, &app_data);
        let suggest = |q: &poster::parse::MediaQuery, reason: &str| suggest(q, reason, &key);

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
            player::ffmpeg_paths::init();
            let dir = app.path().app_data_dir().expect("app data dir");
            std::fs::create_dir_all(&dir).ok();
            migrate::release_seed_if_empty(app, &dir);
            std::fs::create_dir_all(dir.join("covers")).ok();
            std::fs::create_dir_all(dir.join("subtitles")).ok();
            let db = Db::open(&dir.join("collector.sqlite")).expect("open db");
            migrate::migrate_subtitles_to_plain(&dir, &db);
            migrate::migrate_covers_to_subdirs(&dir, &db);
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
            list_media,
            comic::comic_pages,
            comic::comic_page_names,
            comic::comic_page,
            comic::comic_volumes,
            comic::comic_volume_cover,
            player::player_open,
            player::player_stop,
            player::cache_info,
            player::cache_clear,
            launcher::launch_game,
            normalize::normalize_subtitles,
            normalize::subtitle_output_dir,
            normalize::comic_archive::archive_comics_cmd,
            media_update,
            rename_folder,
            move_folder,
            media_create,
            media_delete,
            comic_update,
            comic_delete,
            game_update,
            game_delete,
            import_cover_cropped,
            delete_cover_file,
            set_tmdb_key,
            get_tmdb_key,
            get_subtitle_input_dir,
            set_subtitle_input_dir,
            fetch_posters,
            maintenance::db_reset,
            maintenance::clean_covers
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

    fn seed_game(db: &Db, cpath: &str) {
        let conn = db.0.lock().unwrap();
        conn.execute(
            "INSERT INTO game (category,category_path,title) VALUES ('游戏',?1,?2)",
            rusqlite::params![cpath, format!("g_{cpath}")],
        ).unwrap();
    }
    fn game_cpath_of(db: &Db, title: &str) -> String {
        let conn = db.0.lock().unwrap();
        conn.query_row("SELECT category_path FROM game WHERE title=?1",
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
        rename_folder_in_db(&db, MediaKind::Video, "电影", "科幻", "科幻片").unwrap();
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
        assert!(rename_folder_in_db(&db, MediaKind::Video, "电影", "科幻", "奇幻").is_err());
    }

    #[test]
    fn reject_empty_or_slash_name() {
        let db = Db::open_in_memory().unwrap();
        seed(&db, "电影", "科幻");
        assert!(rename_folder_in_db(&db, MediaKind::Video, "电影", "科幻", "").is_err());
        assert!(rename_folder_in_db(&db, MediaKind::Video, "电影", "科幻", "  ").is_err());
        assert!(rename_folder_in_db(&db, MediaKind::Video, "电影", "科幻", "a/b").is_err());
    }

    #[test]
    fn game_cascade_updates_self_and_descendants() {
        let db = Db::open_in_memory().unwrap();
        seed_game(&db, "角色扮演");
        seed_game(&db, "角色扮演/单机");
        seed_game(&db, "角色扮演续作");
        seed_game(&db, "动作");
        // game 无 category 列，category 参数被忽略；按前缀级联
        rename_folder_in_db(&db, MediaKind::Game, "", "角色扮演", "RPG").unwrap();
        assert_eq!(game_cpath_of(&db, "g_角色扮演"), "RPG");
        assert_eq!(game_cpath_of(&db, "g_角色扮演/单机"), "RPG/单机");
        assert_eq!(game_cpath_of(&db, "g_角色扮演续作"), "角色扮演续作");
        assert_eq!(game_cpath_of(&db, "g_动作"), "动作");
    }

    #[test]
    fn game_reject_sibling_conflict() {
        let db = Db::open_in_memory().unwrap();
        seed_game(&db, "角色扮演");
        seed_game(&db, "动作");
        assert!(rename_folder_in_db(&db, MediaKind::Game, "", "角色扮演", "动作").is_err());
    }

    fn cat_of(db: &Db, title: &str) -> String {
        let conn = db.0.lock().unwrap();
        conn.query_row("SELECT category FROM media WHERE title=?1",
            rusqlite::params![title], |r| r.get(0)).unwrap()
    }

    #[test]
    fn move_leaf_and_target_path() {
        assert_eq!(folder_leaf("科幻"), "科幻");
        assert_eq!(folder_leaf("科幻/诺兰"), "诺兰");
        assert_eq!(folder_leaf("a/b/c"), "c");
        assert_eq!(move_target_path("", "诺兰"), "诺兰");
        assert_eq!(move_target_path("动漫", "诺兰"), "动漫/诺兰");
    }

    #[test]
    fn move_video_cross_category_to_root() {
        let db = Db::open_in_memory().unwrap();
        // 文件夹 "科幻" 及子孙 "科幻/诺兰"，属电影，移到动漫根目录。
        seed(&db, "电影", "科幻");
        seed(&db, "电影", "科幻/诺兰");
        seed(&db, "电影", "科幻小说"); // 非子孙，不应变
        move_folder_in_db(&db, MediaKind::Video, "电影", "科幻", "动漫", "").unwrap();
        // folder_name=科幻, target_parent="" → new_path="科幻"
        assert_eq!(cpath_of(&db, "t_科幻"), "科幻");
        assert_eq!(cat_of(&db, "t_科幻"), "动漫");
        assert_eq!(cpath_of(&db, "t_科幻/诺兰"), "科幻/诺兰");
        assert_eq!(cat_of(&db, "t_科幻/诺兰"), "动漫");
        // 非子孙保持电影不变
        assert_eq!(cpath_of(&db, "t_科幻小说"), "科幻小说");
        assert_eq!(cat_of(&db, "t_科幻小说"), "电影");
    }

    #[test]
    fn move_video_cross_category_into_parent() {
        let db = Db::open_in_memory().unwrap();
        seed(&db, "电影", "科幻/诺兰");
        seed(&db, "电影", "科幻/诺兰/星际"); // 子孙
        move_folder_in_db(&db, MediaKind::Video, "电影", "科幻/诺兰", "动漫", "导演").unwrap();
        // folder_name=诺兰, target_parent="导演" → new_path="导演/诺兰"
        assert_eq!(cpath_of(&db, "t_科幻/诺兰"), "导演/诺兰");
        assert_eq!(cat_of(&db, "t_科幻/诺兰"), "动漫");
        assert_eq!(cpath_of(&db, "t_科幻/诺兰/星际"), "导演/诺兰/星际");
        assert_eq!(cat_of(&db, "t_科幻/诺兰/星际"), "动漫");
    }

    #[test]
    fn move_reject_into_self_or_subtree() {
        let db = Db::open_in_memory().unwrap();
        seed(&db, "电影", "科幻");
        seed(&db, "电影", "科幻/子");
        // 同分类，new_path 落在自身子树下 → Err
        assert!(move_folder_in_db(&db, MediaKind::Video, "电影", "科幻", "电影", "科幻").is_err());
        // 同分类且 new_path == old_path（移回原位）→ Err
        assert!(move_folder_in_db(&db, MediaKind::Video, "电影", "科幻", "电影", "").is_err());
    }

    #[test]
    fn move_reject_target_name_conflict() {
        let db = Db::open_in_memory().unwrap();
        // 源：电影/科幻；目标动漫下已存在同 category_path="科幻" 的行（title 不同以避开 UNIQUE 约束）。
        seed(&db, "电影", "科幻");
        {
            let conn = db.0.lock().unwrap();
            conn.execute(
                "INSERT INTO media (category,category_path,title) VALUES ('动漫','科幻','anime_科幻')",
                [],
            ).unwrap();
        }
        assert!(move_folder_in_db(&db, MediaKind::Video, "电影", "科幻", "动漫", "").is_err());
    }

    #[test]
    fn move_game_same_category() {
        let db = Db::open_in_memory().unwrap();
        seed_game(&db, "角色扮演/单机");
        seed_game(&db, "角色扮演/单机/续作");
        move_folder_in_db(&db, MediaKind::Game, "游戏", "角色扮演/单机", "游戏", "动作").unwrap();
        assert_eq!(game_cpath_of(&db, "g_角色扮演/单机"), "动作/单机");
        assert_eq!(game_cpath_of(&db, "g_角色扮演/单机/续作"), "动作/单机/续作");
    }

    #[test]
    fn move_comic_game_reject_cross_category() {
        let db = Db::open_in_memory().unwrap();
        seed_game(&db, "角色扮演");
        // comic/game target_category != category → Err
        assert!(move_folder_in_db(&db, MediaKind::Game, "游戏", "角色扮演", "别的分类", "").is_err());
    }
}

#[cfg(test)]
mod comic_command_tests {
    use super::*;

    #[test]
    fn comic_update_and_delete() {
        let db = Db::open_in_memory().unwrap();
        let id = {
            let c = db.0.lock().unwrap();
            c.execute("INSERT INTO comic(category,category_path,title,cover_path,description) VALUES('漫画','热血','海贼王',NULL,NULL)", []).unwrap();
            c.last_insert_rowid()
        };
        // update
        update_media_kind_row(&db, "comic", id, "热血", "海贼王改", Some("covers/comic/x.jpg"), Some("简介")).unwrap();
        {
            let c = db.0.lock().unwrap();
            let (t, cp): (String, String) = c.query_row("SELECT title,cover_path FROM comic WHERE id=?1", [id], |r| Ok((r.get(0)?, r.get(1)?))).unwrap();
            assert_eq!(t, "海贼王改");
            assert_eq!(cp, "covers/comic/x.jpg");
        }
        // delete
        delete_media_kind_row(&db, "comic", id).unwrap();
        let n: i64 = db.0.lock().unwrap().query_row("SELECT count(*) FROM comic WHERE id=?1", [id], |r| r.get(0)).unwrap();
        assert_eq!(n, 0);
    }
}
