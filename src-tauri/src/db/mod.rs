pub mod schema;

use crate::error::{AppError, AppResult};
use rusqlite::Connection;
use std::path::Path;
use std::sync::Mutex;

pub struct Db(pub Mutex<Connection>);

impl Db {
    pub fn open(path: &Path) -> AppResult<Self> {
        let conn = Connection::open(path).map_err(|e| AppError::Db(e.to_string()))?;
        conn.execute_batch("PRAGMA foreign_keys = ON;")
            .map_err(|e| AppError::Db(e.to_string()))?;
        for m in schema::MIGRATIONS {
            conn.execute_batch(m).map_err(|e| AppError::Db(e.to_string()))?;
        }
        Ok(Db(Mutex::new(conn)))
    }

    #[cfg(test)]
    pub fn open_in_memory() -> AppResult<Self> {
        let conn = Connection::open_in_memory().map_err(|e| AppError::Db(e.to_string()))?;
        conn.execute_batch("PRAGMA foreign_keys = ON;")
            .map_err(|e| AppError::Db(e.to_string()))?;
        for m in schema::MIGRATIONS {
            conn.execute_batch(m).map_err(|e| AppError::Db(e.to_string()))?;
        }
        Ok(Db(Mutex::new(conn)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn migrations_create_tables() {
        let db = Db::open_in_memory().unwrap();
        let conn = db.0.lock().unwrap();
        let count: i64 = conn
            .query_row(
                "SELECT count(*) FROM sqlite_master WHERE type='table' AND name IN ('media','comic','game','settings')",
                [], |r| r.get(0)).unwrap();
        assert_eq!(count, 4);
    }

    #[test]
    fn foreign_keys_pragma_on() {
        let db = Db::open_in_memory().unwrap();
        let conn = db.0.lock().unwrap();
        let fk: i64 = conn
            .query_row("PRAGMA foreign_keys", [], |r| r.get(0))
            .unwrap();
        assert_eq!(fk, 1);
    }

    // 回归：模拟历史遗留库——comic 表用旧 DDL（无 UNIQUE 约束）建成，
    // 之后 CREATE TABLE IF NOT EXISTS 不再重建它。补偿迁移的唯一索引应让
    // `ON CONFLICT(category_path,title)` upsert 可用，不再报 "does not match ...
    // UNIQUE constraint"。
    #[test]
    fn upsert_works_on_legacy_comic_table_without_unique() {
        let conn = Connection::open_in_memory().unwrap();
        // 先建一个缺约束的旧 comic 表
        conn.execute_batch(
            "CREATE TABLE comic (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                category TEXT NOT NULL,
                category_path TEXT NOT NULL,
                title TEXT NOT NULL,
                description TEXT,
                cover_path TEXT
            );",
        )
        .unwrap();
        // 跑全部迁移（IF NOT EXISTS 跳过已存在的 comic，仅补建唯一索引）
        for m in schema::MIGRATIONS {
            conn.execute_batch(m).unwrap();
        }
        // upsert 两次同键，应命中 DO UPDATE 而非报错
        let upsert = "INSERT INTO comic(category,category_path,title,cover_path,description)
                      VALUES(?1,?2,?3,NULL,NULL)
                      ON CONFLICT(category_path,title) DO UPDATE SET category=excluded.category";
        conn.execute(upsert, rusqlite::params!["热血", "热血/海贼王", "第01卷"])
            .unwrap();
        conn.execute(upsert, rusqlite::params!["热血2", "热血/海贼王", "第01卷"])
            .expect("唯一索引应作为 upsert 冲突目标，不应报错");
        let (n, cat): (i64, String) = conn
            .query_row(
                "SELECT count(*), max(category) FROM comic WHERE category_path='热血/海贼王'",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!(n, 1); // 未插入重复行
        assert_eq!(cat, "热血2"); // 而是更新了原行
    }
}
