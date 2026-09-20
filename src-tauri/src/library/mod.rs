pub mod model;
pub mod paths;
pub mod scanner;

use crate::db::Db;
use crate::error::{AppError, AppResult};
use crate::library::model::{MediaItem, MediaKind};
use crate::library::scanner::ScannedItem;
use rusqlite::params;

/// 按 id 更新一条视频条目的可编辑字段。仅作用于 media 表（视频编辑专用）。
pub fn update_item(db: &Db, id: i64, it: &ScannedItem) -> AppResult<()> {
    let conn = db.0.lock().unwrap();
    conn.execute(
        "UPDATE media SET category=?1, category_path=?2, title=?3,
           cover_path=?4, description=?5 WHERE id=?6",
        params![it.category, it.category_path, it.title,
                it.cover_path, it.description, id],
    ).map_err(|e| AppError::Db(e.to_string()))?;
    Ok(())
}

/// 按 id 删除 media 表的一条条目（视频删除专用）。comic/game 的删除由 lib.rs 的 delete_media_kind_row 处理。
pub fn delete_item(db: &Db, id: i64) -> AppResult<()> {
    let conn = db.0.lock().unwrap();
    conn.execute("DELETE FROM media WHERE id=?1", params![id])
        .map_err(|e| AppError::Db(e.to_string()))?;
    Ok(())
}

/// 新增一条条目，返回新 id。表名由 `kind.table_name()` 决定。
/// 三表列结构一致；video 用传入的 category，comic/game 的 category 固定为「漫画」「游戏」。
pub fn create_item(db: &Db, kind: MediaKind, it: &ScannedItem) -> AppResult<i64> {
    let conn = db.0.lock().unwrap();
    let table = kind.table_name();
    match kind {
        MediaKind::Video => {
            conn.execute(
                &format!(
                    "INSERT INTO {table}
                      (category,category_path,title,cover_path,description)
                     VALUES (?1,?2,?3,?4,?5)"
                ),
                params![
                    it.category,
                    it.category_path,
                    it.title,
                    it.cover_path,
                    it.description
                ],
            )
            .map_err(|e| AppError::Db(e.to_string()))?;
        }
        MediaKind::Comic => {
            conn.execute(
                "INSERT INTO comic
                  (category,category_path,title,cover_path,description)
                 VALUES ('漫画',?1,?2,?3,?4)",
                params![it.category_path, it.title, it.cover_path, it.description],
            )
            .map_err(|e| AppError::Db(e.to_string()))?;
        }
        MediaKind::Game => {
            conn.execute(
                "INSERT INTO game
                  (category,category_path,title,cover_path,description)
                 VALUES ('游戏',?1,?2,?3,?4)",
                params![it.category_path, it.title, it.cover_path, it.description],
            )
            .map_err(|e| AppError::Db(e.to_string()))?;
        }
    }
    Ok(conn.last_insert_rowid())
}

/// 从 comic/game 这类表读取条目（列结构一致，category 由 category_path 首级派生）。
/// `table` 只来自内部常量 "comic"/"game"，无注入风险，用 format! 拼表名。
fn list_kind_rows(conn: &rusqlite::Connection, table: &str) -> AppResult<Vec<MediaItem>> {
    let sql = format!(
        "SELECT id,category_path,title,cover_path,description
         FROM {table} ORDER BY category_path, title"
    );
    let mut stmt = conn.prepare(&sql).map_err(|e| AppError::Db(e.to_string()))?;
    let rows = stmt
        .query_map([], |r| {
            let category_path: String = r.get(1)?;
            let category = category_path.split('/').next().unwrap_or("").to_string();
            Ok(MediaItem {
                id: r.get(0)?,
                category,
                category_path,
                title: r.get(2)?,
                cover_path: r.get(3)?,
                description: r.get(4)?,
                playable: false,
                video_path: String::new(),
            })
        })
        .map_err(|e| AppError::Db(e.to_string()))?;
    let mut out = Vec::new();
    for row in rows {
        out.push(row.map_err(|e| AppError::Db(e.to_string()))?);
    }
    Ok(out)
}

pub fn list_items(db: &Db, kind: MediaKind) -> AppResult<Vec<MediaItem>> {
    let conn = db.0.lock().unwrap();
    let mut out = Vec::new();
    match kind {
        MediaKind::Video => {
            let mut stmt = conn
                .prepare(
                    "SELECT id,category,category_path,title,cover_path,description
                     FROM media ORDER BY category_path, title",
                )
                .map_err(|e| AppError::Db(e.to_string()))?;
            let rows = stmt
                .query_map([], |r| {
                    Ok(MediaItem {
                        id: r.get(0)?,
                        category: r.get(1)?,
                        category_path: r.get(2)?,
                        title: r.get(3)?,
                        cover_path: r.get(4)?,
                        description: r.get(5)?,
                        playable: false,
                        video_path: String::new(),
                    })
                })
                .map_err(|e| AppError::Db(e.to_string()))?;
            for row in rows {
                out.push(row.map_err(|e| AppError::Db(e.to_string()))?);
            }
        }
        MediaKind::Comic => {
            out.extend(list_kind_rows(&conn, "comic")?);
        }
        MediaKind::Game => {
            out.extend(list_kind_rows(&conn, "game")?);
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::library::scanner::ScannedItem;

    // 参数作 title 用（去重键 (category_path,title) 的区分维度），保证批内条目唯一。
    fn sample(title: &str) -> ScannedItem {
        ScannedItem {
            category: "电影".into(),
            category_path: "电影/科幻".into(),
            title: title.into(),
            cover_path: None,
            description: None,
        }
    }

    #[test]
    fn update_changes_fields() {
        let db = Db::open_in_memory().unwrap();
        create_item(&db, MediaKind::Video, &sample("/a.mkv")).unwrap();
        let id: i64 = { let c = db.0.lock().unwrap();
            c.query_row("SELECT id FROM media LIMIT 1", [], |r| r.get(0)).unwrap() };
        let mut it = sample("/a.mkv"); it.title = "新标题".into();
        update_item(&db, id, &it).unwrap();
        let items = list_items(&db, MediaKind::Video).unwrap();
        assert_eq!(items[0].title, "新标题");
    }
    #[test]
    fn delete_removes() {
        let db = Db::open_in_memory().unwrap();
        let id = create_item(&db, MediaKind::Video, &sample("/a.mkv")).unwrap();
        delete_item(&db, id).unwrap();
        assert_eq!(list_items(&db, MediaKind::Video).unwrap().len(), 0);
    }
    #[test]
    fn create_returns_incrementing_id() {
        let db = Db::open_in_memory().unwrap();
        let id1 = create_item(&db, MediaKind::Video, &sample("/a.mkv")).unwrap();
        let id2 = create_item(&db, MediaKind::Video, &sample("/b.mkv")).unwrap();
        assert!(id2 > id1);
        assert_eq!(list_items(&db, MediaKind::Video).unwrap().len(), 2);
    }
}
