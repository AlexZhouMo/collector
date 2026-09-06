pub mod model;
pub mod scanner;

use crate::db::Db;
use crate::error::{AppError, AppResult};
use crate::library::model::{MediaItem, MediaKind};
use crate::library::scanner::ScannedItem;
use rusqlite::params;

pub fn upsert_items(db: &Db, items: &[ScannedItem]) -> AppResult<usize> {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;
    let mut conn = db.0.lock().unwrap();
    let tx = conn.transaction().map_err(|e| AppError::Db(e.to_string()))?;
    let mut n = 0;
    for it in items {
        tx.execute(
            "INSERT INTO media_item
              (kind,category,category_path,title,path,subtitle_path,cover_path,description,platform_ok,exec_path,scanned_at)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11)
             ON CONFLICT(path) DO UPDATE SET
               category=excluded.category, category_path=excluded.category_path,
               title=excluded.title, subtitle_path=excluded.subtitle_path,
               cover_path=excluded.cover_path, description=excluded.description,
               platform_ok=excluded.platform_ok, exec_path=excluded.exec_path,
               scanned_at=excluded.scanned_at",
            params![
                it.kind.as_str(), it.category, it.category_path, it.title, it.path,
                it.subtitle_path, it.cover_path, it.description,
                it.platform_ok as i64, it.exec_path, now
            ],
        )
        .map_err(|e| AppError::Db(e.to_string()))?;
        n += 1;
    }
    tx.commit().map_err(|e| AppError::Db(e.to_string()))?;
    Ok(n)
}

pub fn list_items(db: &Db, kind: MediaKind) -> AppResult<Vec<MediaItem>> {
    let conn = db.0.lock().unwrap();
    let mut stmt = conn
        .prepare(
            "SELECT id,kind,category,category_path,title,path,subtitle_path,cover_path,description,platform_ok,exec_path
         FROM media_item WHERE kind=?1 ORDER BY category_path, title",
        )
        .map_err(|e| AppError::Db(e.to_string()))?;
    let rows = stmt
        .query_map(params![kind.as_str()], |r| {
            let kind_s: String = r.get(1)?;
            let kind = match kind_s.as_str() {
                "video" => MediaKind::Video,
                "comic" => MediaKind::Comic,
                _ => MediaKind::Game,
            };
            Ok(MediaItem {
                id: r.get(0)?,
                kind,
                category: r.get(2)?,
                category_path: r.get(3)?,
                title: r.get(4)?,
                path: r.get(5)?,
                subtitle_path: r.get(6)?,
                cover_path: r.get(7)?,
                description: r.get(8)?,
                platform_ok: r.get::<_, i64>(9)? != 0,
                exec_path: r.get(10)?,
            })
        })
        .map_err(|e| AppError::Db(e.to_string()))?;
    let mut out = Vec::new();
    for row in rows {
        out.push(row.map_err(|e| AppError::Db(e.to_string()))?);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::library::scanner::ScannedItem;

    fn sample(path: &str) -> ScannedItem {
        ScannedItem {
            kind: MediaKind::Video,
            category: "电影".into(),
            category_path: "电影/科幻".into(),
            title: "T".into(),
            path: path.into(),
            subtitle_path: None,
            cover_path: None,
            description: None,
            platform_ok: true,
            exec_path: None,
        }
    }

    #[test]
    fn upsert_then_list_roundtrip_and_dedup() {
        let db = Db::open_in_memory().unwrap();
        upsert_items(&db, &[sample("/a.mkv"), sample("/b.mkv")]).unwrap();
        upsert_items(&db, &[sample("/a.mkv")]).unwrap(); // 同 path 应更新而非重复
        let items = list_items(&db, MediaKind::Video).unwrap();
        assert_eq!(items.len(), 2);
    }
}
