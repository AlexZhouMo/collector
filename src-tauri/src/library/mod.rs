pub mod cover;
pub mod model;
pub mod paths;
pub mod scanner;

use crate::db::Db;
use crate::error::{AppError, AppResult};
use crate::library::model::{MediaItem, MediaKind};
use crate::library::scanner::ScannedItem;
use rusqlite::params;

/// 重扫入库：在同一事务内先清空该 kind 对应表的所有旧记录，再插入当前扫到的。
/// 用于"每次扫描重建目录结构、清除已不存在的幽灵条目"。
/// `kind` 决定入库表名（media/comic/game）；items 可能为空（该类型清空为无）。
pub fn replace_items(db: &Db, kind: MediaKind, items: &[ScannedItem]) -> AppResult<usize> {
    // 按 (category_path, title) 排序后逐条顺序插入，使 id 与展示顺序一致。
    let mut sorted: Vec<&ScannedItem> = items.iter().collect();
    sorted.sort_by(|a, b| {
        a.category_path
            .cmp(&b.category_path)
            .then_with(|| a.title.cmp(&b.title))
    });
    let table = kind.table_name();
    let mut conn = db.0.lock().unwrap();
    let tx = conn.transaction().map_err(|e| AppError::Db(e.to_string()))?;
    // 整表清空（表按 kind 隔离，不再按 kind 列过滤）。
    tx.execute(&format!("DELETE FROM {table}"), [])
        .map_err(|e| AppError::Db(e.to_string()))?;
    // 重置 AUTOINCREMENT 计数，使 id 从 1 重新开始（该行不存在时为 no-op）。
    tx.execute(
        &format!("DELETE FROM sqlite_sequence WHERE name='{table}'"),
        [],
    )
    .map_err(|e| AppError::Db(e.to_string()))?;
    let mut n = 0;
    for it in &sorted {
        insert_one_tx(&tx, kind, it)?;
        n += 1;
    }
    tx.commit().map_err(|e| AppError::Db(e.to_string()))?;
    Ok(n)
}

/// 在给定事务内插入单条条目（去重键 (category_path,title) 冲突则更新）。供 replace_items 复用。
/// 表名由 `kind.table_name()` 决定；media 表含 subtitle_path 列，comic/game 无。
/// 表名来自枚举（非用户输入），format! 拼接无注入风险。
fn insert_one_tx(
    tx: &rusqlite::Transaction<'_>,
    kind: MediaKind,
    it: &ScannedItem,
) -> AppResult<()> {
    let table = kind.table_name();
    match kind {
        MediaKind::Video => {
            tx.execute(
                &format!(
                    "INSERT INTO {table}
                      (category,category_path,title,cover_path,description)
                     VALUES (?1,?2,?3,?4,?5)
                     ON CONFLICT(category_path,title) DO UPDATE SET
                       category=excluded.category,
                       cover_path=excluded.cover_path, description=excluded.description"
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
        MediaKind::Comic | MediaKind::Game => {
            tx.execute(
                &format!(
                    "INSERT INTO {table}
                      (category,category_path,title,cover_path,description)
                     VALUES (?1,?2,?3,?4,?5)
                     ON CONFLICT(category_path,title) DO UPDATE SET
                       category=excluded.category,
                       cover_path=excluded.cover_path, description=excluded.description"
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
    }
    Ok(())
}

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

/// 按 id 删除一条条目。仅作用于 media 表（视频删除专用；comic/game 暂无删除需求）。
pub fn delete_item(db: &Db, id: i64) -> AppResult<()> {
    let conn = db.0.lock().unwrap();
    conn.execute("DELETE FROM media WHERE id=?1", params![id])
        .map_err(|e| AppError::Db(e.to_string()))?;
    Ok(())
}

/// 新增一条条目，返回新 id。表名由 `kind.table_name()` 决定。
/// media 表含 subtitle_path 列，comic/game 无。
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
        MediaKind::Comic | MediaKind::Game => {
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
    }
    Ok(conn.last_insert_rowid())
}

pub fn list_items(db: &Db, kind: MediaKind) -> AppResult<Vec<MediaItem>> {
    let conn = db.0.lock().unwrap();
    let table = kind.table_name();
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
        MediaKind::Comic | MediaKind::Game => {
            let mut stmt = conn
                .prepare(&format!(
                    "SELECT id,category,category_path,title,cover_path,description
                     FROM {table} ORDER BY category_path, title"
                ))
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
    fn replace_dedups_within_batch() {
        let db = Db::open_in_memory().unwrap();
        replace_items(&db, MediaKind::Video, &[sample("/a.mkv"), sample("/b.mkv")]).unwrap();
        let items = list_items(&db, MediaKind::Video).unwrap();
        assert_eq!(items.len(), 2);
    }

    #[test]
    fn replace_clears_stale_entries() {
        let db = Db::open_in_memory().unwrap();
        // 首次扫到 a、b
        replace_items(&db, MediaKind::Video, &[sample("/a.mkv"), sample("/b.mkv")]).unwrap();
        assert_eq!(list_items(&db, MediaKind::Video).unwrap().len(), 2);
        // 重扫只剩 a（b 已从磁盘删除）→ 库里应只剩 a，幽灵条目 b 被清除
        replace_items(&db, MediaKind::Video, &[sample("/a.mkv")]).unwrap();
        let items = list_items(&db, MediaKind::Video).unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].title, "/a.mkv");
    }

    #[test]
    fn replace_resets_id_to_one_and_orders() {
        let db = Db::open_in_memory().unwrap();
        replace_items(&db, MediaKind::Video, &[sample("/a.mkv"), sample("/b.mkv")]).unwrap();
        let mut s1 = sample("/x.mkv"); s1.category_path = "电影/z".into(); s1.title = "Z".into();
        let mut s2 = sample("/y.mkv"); s2.category_path = "电影/a".into(); s2.title = "A".into();
        replace_items(&db, MediaKind::Video, &[s1, s2]).unwrap();
        let conn = db.0.lock().unwrap();
        let (min_id, max_id): (i64, i64) = conn
            .query_row("SELECT min(id), max(id) FROM media", [], |r| Ok((r.get(0)?, r.get(1)?)))
            .unwrap();
        assert_eq!(min_id, 1);
        assert_eq!(max_id, 2);
        let first_title: String = conn
            .query_row("SELECT title FROM media WHERE id=1", [], |r| r.get(0)).unwrap();
        assert_eq!(first_title, "A"); // 电影/a 排在 电影/z 前
    }

    #[test]
    fn replace_only_affects_its_kind() {
        let db = Db::open_in_memory().unwrap();
        let comic = sample("/c.zip");
        replace_items(&db, MediaKind::Video, &[sample("/a.mkv")]).unwrap();
        replace_items(&db, MediaKind::Comic, &[comic]).unwrap();
        // 重扫 video 不应清掉 comic
        replace_items(&db, MediaKind::Video, &[sample("/a.mkv")]).unwrap();
        assert_eq!(list_items(&db, MediaKind::Video).unwrap().len(), 1);
        assert_eq!(list_items(&db, MediaKind::Comic).unwrap().len(), 1);
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
