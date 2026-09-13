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
    // 之后经"删 category 列"迁移重建为带 UNIQUE(category_path,title) 的新表。
    // 迁移后 `ON CONFLICT(category_path,title)` upsert 应可用（不再报
    // "does not match ... UNIQUE constraint"），且不含 category 列。
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
        // 跑全部迁移（重建 comic 为无 category、带唯一约束的新表）
        for m in schema::MIGRATIONS {
            conn.execute_batch(m).unwrap();
        }
        // upsert 两次同键，应命中 DO UPDATE 而非报错
        let upsert = "INSERT INTO comic(category_path,title,cover_path,description)
                      VALUES(?1,?2,NULL,NULL)
                      ON CONFLICT(category_path,title) DO UPDATE SET description=excluded.description";
        conn.execute(upsert, rusqlite::params!["热血/海贼王", "第01卷"])
            .unwrap();
        conn.execute(upsert, rusqlite::params!["热血/海贼王", "第01卷"])
            .expect("唯一约束应作为 upsert 冲突目标，不应报错");
        let n: i64 = conn
            .query_row(
                "SELECT count(*) FROM comic WHERE category_path='热血/海贼王'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(n, 1); // 未插入重复行，而是命中更新
    }

    #[test]
    fn migrates_legacy_comic_drops_category() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE comic (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                category TEXT NOT NULL,
                category_path TEXT NOT NULL,
                title TEXT NOT NULL,
                description TEXT,
                cover_path TEXT,
                UNIQUE(category_path,title)
            );",
        ).unwrap();
        conn.execute("INSERT INTO comic(category,category_path,title) VALUES('x','热血','灌篮高手')", []).unwrap();
        for m in schema::MIGRATIONS {
            conn.execute_batch(m).unwrap();
        }
        let has_category: bool = conn
            .prepare("SELECT 1 FROM pragma_table_info('comic') WHERE name='category'")
            .unwrap().exists([]).unwrap();
        assert!(!has_category, "comic.category 应已删除");
        // 迁移已把旧行 (热血,灌篮高手) 拷入新表，故换一组未占用的键验证 UNIQUE 仍在。
        conn.execute("INSERT INTO comic(category_path,title) VALUES('热血','海贼王')", []).unwrap();
        let dup = conn.execute("INSERT INTO comic(category_path,title) VALUES('热血','海贼王')", []);
        assert!(dup.is_err(), "UNIQUE(category_path,title) 应拦截重复");
    }

    #[test]
    fn migrates_legacy_game_drops_category() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE game (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                category TEXT NOT NULL,
                category_path TEXT NOT NULL,
                title TEXT NOT NULL,
                description TEXT,
                cover_path TEXT,
                UNIQUE(category_path,title)
            );",
        ).unwrap();
        conn.execute("INSERT INTO game(category,category_path,title) VALUES('x','无双系列/真三国无双','真三国无双5')", []).unwrap();
        for m in schema::MIGRATIONS {
            conn.execute_batch(m).unwrap();
        }
        let has_category: bool = conn
            .prepare("SELECT 1 FROM pragma_table_info('game') WHERE name='category'")
            .unwrap().exists([]).unwrap();
        assert!(!has_category, "game.category 应已删除");
        // 旧数据应无损迁入新表
        let kept: i64 = conn
            .query_row("SELECT count(*) FROM game WHERE category_path='无双系列/真三国无双' AND title='真三国无双5'", [], |r| r.get(0))
            .unwrap();
        assert_eq!(kept, 1, "旧 game 行应无损迁入");
        // 换一组未占用键验证 UNIQUE 仍在
        conn.execute("INSERT INTO game(category_path,title) VALUES('策略对战','三国志11')", []).unwrap();
        let dup = conn.execute("INSERT INTO game(category_path,title) VALUES('策略对战','三国志11')", []);
        assert!(dup.is_err(), "UNIQUE(category_path,title) 应拦截重复");
    }
}
