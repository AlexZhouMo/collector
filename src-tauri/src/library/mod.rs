pub mod cover;
pub mod model;
pub mod paths;
pub mod subtitle;
pub mod scanner;

use crate::db::Db;
use crate::error::{AppError, AppResult};
use crate::library::model::{MediaItem, MediaKind};
use crate::library::scanner::ScannedItem;
use rusqlite::params;

/// 重扫入库：在同一事务内先删掉该 kind 的所有旧记录（watch_state/game_state
/// 随外键 CASCADE 一并清空，即观看进度/页码/启动次数重置），再插入当前扫到的。
/// 用于"每次扫描重建目录结构、清除已不存在的幽灵条目"。
/// `kind` 取 items 的类型；items 可能为空（该类型清空为无）。
pub fn replace_items(db: &Db, kind: MediaKind, items: &[ScannedItem]) -> AppResult<usize> {
    // 按 (category_path, title) 排序后逐条顺序插入，使 id 与展示顺序一致。
    let mut sorted: Vec<&ScannedItem> = items.iter().collect();
    sorted.sort_by(|a, b| {
        a.category_path
            .cmp(&b.category_path)
            .then_with(|| a.title.cmp(&b.title))
    });
    let mut conn = db.0.lock().unwrap();
    let tx = conn.transaction().map_err(|e| AppError::Db(e.to_string()))?;
    tx.execute(
        "DELETE FROM media_item WHERE kind=?1",
        params![kind.as_str()],
    )
    .map_err(|e| AppError::Db(e.to_string()))?;
    // 重置 AUTOINCREMENT 计数，使 id 从 1 重新开始（该行不存在时为 no-op）。
    tx.execute("DELETE FROM sqlite_sequence WHERE name='media_item'", [])
        .map_err(|e| AppError::Db(e.to_string()))?;
    let mut n = 0;
    for it in &sorted {
        insert_one_tx(&tx, it)?;
        n += 1;
    }
    tx.commit().map_err(|e| AppError::Db(e.to_string()))?;
    Ok(n)
}

/// 在给定事务内插入单条条目（path 冲突则更新）。供 replace_items 复用。
fn insert_one_tx(
    tx: &rusqlite::Transaction<'_>,
    it: &ScannedItem,
) -> AppResult<()> {
    tx.execute(
        "INSERT INTO media_item
          (kind,category,category_path,title,path,subtitle_path,cover_path,description,platform_ok,exec_path)
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10)
         ON CONFLICT(path) DO UPDATE SET
           category=excluded.category, category_path=excluded.category_path,
           title=excluded.title, subtitle_path=excluded.subtitle_path,
           cover_path=excluded.cover_path, description=excluded.description,
           platform_ok=excluded.platform_ok, exec_path=excluded.exec_path",
        params![
            it.kind.as_str(), it.category, it.category_path, it.title, it.path,
            it.subtitle_path, it.cover_path, it.description,
            it.platform_ok as i64, it.exec_path
        ],
    )
    .map_err(|e| AppError::Db(e.to_string()))?;
    Ok(())
}

/// 按 id 更新一条视频条目的可编辑字段（kind 不变）。
pub fn update_item(db: &Db, id: i64, it: &ScannedItem) -> AppResult<()> {
    let conn = db.0.lock().unwrap();
    conn.execute(
        "UPDATE media_item SET category=?1, category_path=?2, title=?3, path=?4,
           subtitle_path=?5, cover_path=?6, description=?7 WHERE id=?8",
        params![it.category, it.category_path, it.title, it.path,
                it.subtitle_path, it.cover_path, it.description, id],
    ).map_err(|e| AppError::Db(e.to_string()))?;
    Ok(())
}

/// 按 id 删除一条条目（watch_state 随外键 CASCADE 一并删）。
pub fn delete_item(db: &Db, id: i64) -> AppResult<()> {
    let conn = db.0.lock().unwrap();
    conn.execute("DELETE FROM media_item WHERE id=?1", params![id])
        .map_err(|e| AppError::Db(e.to_string()))?;
    Ok(())
}

/// 新增一条条目，返回新 id。
pub fn create_item(db: &Db, it: &ScannedItem) -> AppResult<i64> {
    let conn = db.0.lock().unwrap();
    conn.execute(
        "INSERT INTO media_item
          (kind,category,category_path,title,path,subtitle_path,cover_path,description,platform_ok,exec_path)
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10)",
        params![it.kind.as_str(), it.category, it.category_path, it.title, it.path,
                it.subtitle_path, it.cover_path, it.description, it.platform_ok as i64, it.exec_path],
    ).map_err(|e| AppError::Db(e.to_string()))?;
    Ok(conn.last_insert_rowid())
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
        assert_eq!(items[0].path, "/a.mkv");
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
            .query_row("SELECT min(id), max(id) FROM media_item", [], |r| Ok((r.get(0)?, r.get(1)?)))
            .unwrap();
        assert_eq!(min_id, 1);
        assert_eq!(max_id, 2);
        let first_title: String = conn
            .query_row("SELECT title FROM media_item WHERE id=1", [], |r| r.get(0)).unwrap();
        assert_eq!(first_title, "A"); // 电影/a 排在 电影/z 前
    }

    #[test]
    fn replace_only_affects_its_kind() {
        let db = Db::open_in_memory().unwrap();
        let mut comic = sample("/c.zip");
        comic.kind = MediaKind::Comic;
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
        create_item(&db, &sample("/a.mkv")).unwrap();
        let id: i64 = { let c = db.0.lock().unwrap();
            c.query_row("SELECT id FROM media_item LIMIT 1", [], |r| r.get(0)).unwrap() };
        let mut it = sample("/a.mkv"); it.title = "新标题".into();
        update_item(&db, id, &it).unwrap();
        let items = list_items(&db, MediaKind::Video).unwrap();
        assert_eq!(items[0].title, "新标题");
    }
    #[test]
    fn delete_removes() {
        let db = Db::open_in_memory().unwrap();
        let id = create_item(&db, &sample("/a.mkv")).unwrap();
        delete_item(&db, id).unwrap();
        assert_eq!(list_items(&db, MediaKind::Video).unwrap().len(), 0);
    }
    #[test]
    fn create_returns_incrementing_id() {
        let db = Db::open_in_memory().unwrap();
        let id1 = create_item(&db, &sample("/a.mkv")).unwrap();
        let id2 = create_item(&db, &sample("/b.mkv")).unwrap();
        assert!(id2 > id1);
        assert_eq!(list_items(&db, MediaKind::Video).unwrap().len(), 2);
    }
}
