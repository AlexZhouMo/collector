use crate::db::Db;
use std::path::Path;
use tauri::Manager;

pub(crate) fn migrate_subtitles_to_plain(app_data: &Path, db: &Db) {
    let conn = db.0.lock().unwrap();
    let has_col: bool = conn
        .prepare("SELECT 1 FROM pragma_table_info('media') WHERE name='subtitle_path'")
        .and_then(|mut s| s.exists([]))
        .unwrap_or(false);
    if !has_col { return; }
    let app_data_str = app_data.to_string_lossy();
    let db_path = app_data.join("collector.sqlite");
    let bak = app_data.join("collector.sqlite.bak");
    if !bak.exists() { let _ = std::fs::copy(&db_path, &bak); }
    let rows: Vec<(String, String, String, String)> = {
        let mut stmt = match conn.prepare(
            "SELECT category, category_path, title, subtitle_path FROM media \
             WHERE subtitle_path IS NOT NULL AND trim(subtitle_path) <> ''") {
            Ok(s) => s, Err(e) => { eprintln!("[migrate] prepare 失败: {e}"); return; }
        };
        let mapped = stmt.query_map([], |r| Ok((
            r.get::<_, String>(0)?, r.get::<_, String>(1)?,
            r.get::<_, String>(2)?, r.get::<_, String>(3)?,
        )));
        match mapped {
            Ok(it) => it.filter_map(|x| x.ok()).collect(),
            Err(e) => { eprintln!("[migrate] query 失败: {e}"); return; }
        }
    };
    let mut moved = 0usize;
    for (category, cpath, title, rel) in &rows {
        let src = crate::library::paths::appdata_to_absolute(rel, &app_data_str);
        let dst = crate::library::paths::subtitle_abs_path(&app_data_str, category, cpath, title);
        if src == dst { continue; }
        if !std::path::Path::new(&src).is_file() { continue; }
        if let Some(parent) = std::path::Path::new(&dst).parent() {
            if std::fs::create_dir_all(parent).is_err() { eprintln!("[migrate] 建目录失败: {dst}"); continue; }
        }
        match std::fs::rename(&src, &dst) {
            Ok(_) => moved += 1,
            Err(e) => eprintln!("[migrate] 移动失败 {src} -> {dst}: {e}"),
        }
    }
    if let Err(e) = conn.execute("ALTER TABLE media DROP COLUMN subtitle_path", []) {
        eprintln!("[migrate] DROP COLUMN 失败: {e}");
    } else {
        eprintln!("[migrate] 完成：归位 {moved} 条字幕，已删除 subtitle_path 列");
    }
}

/// 启动幂等迁移：把 covers/ 根下扁平封面按前缀移入子目录（cover_/tmdb_→media）。
/// 并更新 DB cover_path。已在子目录/已迁移的跳过；失败记日志跳过，不中断启动。
pub(crate) fn migrate_covers_to_subdirs(app_data: &Path, db: &Db) {
    let covers = app_data.join("covers");
    let rd = match std::fs::read_dir(&covers) { Ok(r) => r, Err(_) => return };
    for e in rd.filter_map(|e| e.ok()) {
        let p = e.path();
        if !p.is_file() { continue; } // 跳过 media/comic/game 子目录
        let name = match p.file_name().and_then(|s| s.to_str()) { Some(n) => n.to_string(), None => continue };
        let sub = if name.starts_with("cover_") || name.starts_with("tmdb_") { "media" }
                  else { continue };
        let dest_dir = covers.join(sub);
        if std::fs::create_dir_all(&dest_dir).is_err() { continue; }
        let dest = dest_dir.join(&name);
        if std::fs::rename(&p, &dest).is_err() { continue; }
        let old_rel = format!("covers/{name}");
        let new_rel = format!("covers/{sub}/{name}");
        if let Ok(conn) = db.0.lock() {
            let _ = conn.execute("UPDATE media SET cover_path=?1 WHERE cover_path=?2", rusqlite::params![new_rel, old_rel]);
        }
    }
}

/// 首启释放：若用户数据目录无 collector.sqlite，则把 bundled seed（数据库+covers+subtitles）
/// 递归复制到 app_data。已存在则跳过（不覆盖用户数据）。seed 缺失（如无 bundle）静默跳过。
pub(crate) fn release_seed_if_empty(app: &tauri::App, app_data: &Path) {
    if app_data.join("collector.sqlite").exists() {
        return; // 已有数据，不覆盖
    }
    let seed = match app.path().resource_dir() {
        Ok(r) => r.join("seed"),
        Err(_) => return,
    };
    if !seed.exists() { return; }
    fn copy_dir(src: &std::path::Path, dst: &std::path::Path) -> std::io::Result<()> {
        std::fs::create_dir_all(dst)?;
        for e in std::fs::read_dir(src)? {
            let e = e?; let p = e.path(); let d = dst.join(e.file_name());
            if p.is_dir() { copy_dir(&p, &d)?; } else { std::fs::copy(&p, &d)?; }
        }
        Ok(())
    }
    if let Err(e) = copy_dir(&seed, app_data) {
        eprintln!("[seed] 释放失败: {e}");
    }
}

#[cfg(test)]
mod tests {
    use super::{migrate_covers_to_subdirs, migrate_subtitles_to_plain};
    use crate::db::Db;

    #[test]
    fn migrate_moves_hash_subs_and_drops_column() {
        use std::fs;
        let tmp = tempfile::tempdir().unwrap();
        let app_data = tmp.path();
        fs::create_dir_all(app_data.join("subtitles")).unwrap();
        let db_path = app_data.join("collector.sqlite");
        {
            let conn = rusqlite::Connection::open(&db_path).unwrap();
            conn.execute_batch("CREATE TABLE media (id INTEGER PRIMARY KEY AUTOINCREMENT, category TEXT, category_path TEXT, title TEXT, description TEXT, subtitle_path TEXT, cover_path TEXT, UNIQUE(category_path,title));").unwrap();
            conn.execute("INSERT INTO media (category,category_path,title,subtitle_path) VALUES ('电影','科幻','星战','subtitles/sub_abc.ass')", []).unwrap();
        }
        fs::write(app_data.join("subtitles/sub_abc.ass"), "x").unwrap();
        let db = crate::db::Db::open(&db_path).unwrap();
        migrate_subtitles_to_plain(app_data, &db);
        assert!(app_data.join("subtitles/电影/科幻/星战.ass").is_file());
        assert!(!app_data.join("subtitles/sub_abc.ass").exists());
        let conn = db.0.lock().unwrap();
        let has: bool = conn.prepare("SELECT 1 FROM pragma_table_info('media') WHERE name='subtitle_path'")
            .and_then(|mut s| s.exists([])).unwrap_or(false);
        assert!(!has);
        assert!(app_data.join("collector.sqlite.bak").exists());
    }

    #[test]
    fn migrate_covers_moves_by_prefix_and_updates_db() {
        let tmp = tempfile::tempdir().unwrap();
        let app_data = tmp.path();
        let covers = app_data.join("covers");
        std::fs::create_dir_all(&covers).unwrap();
        // 扁平封面文件
        std::fs::write(covers.join("cover_aaa.jpg"), b"x").unwrap();
        std::fs::write(covers.join("tmdb_bbb.jpg"), b"x").unwrap();
        let db = Db::open_in_memory().unwrap();
        {
            let c = db.0.lock().unwrap();
            c.execute("INSERT INTO media(category,category_path,title,cover_path) VALUES('电影','电影','A','covers/cover_aaa.jpg')", []).unwrap();
            c.execute("INSERT INTO media(category,category_path,title,cover_path) VALUES('电影','电影','B','covers/tmdb_bbb.jpg')", []).unwrap();
        }
        migrate_covers_to_subdirs(app_data, &db);
        // 文件移到子目录
        assert!(covers.join("media/cover_aaa.jpg").is_file());
        assert!(covers.join("media/tmdb_bbb.jpg").is_file());
        assert!(!covers.join("cover_aaa.jpg").exists());
        // DB 更新
        let c = db.0.lock().unwrap();
        let a: String = c.query_row("SELECT cover_path FROM media WHERE title='A'", [], |r| r.get(0)).unwrap();
        assert_eq!(a, "covers/media/cover_aaa.jpg");
        drop(c);
        // 幂等重跑不报错、不重复
        migrate_covers_to_subdirs(app_data, &db);
        assert!(covers.join("media/cover_aaa.jpg").is_file());
    }
}
