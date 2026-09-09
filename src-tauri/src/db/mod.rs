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
                "SELECT count(*) FROM sqlite_master WHERE type='table' AND name IN ('media_item','watch_state','game_state','settings')",
                [], |r| r.get(0)).unwrap();
        assert_eq!(count, 4);
    }

    #[test]
    fn foreign_keys_cascade_delete() {
        let db = Db::open_in_memory().unwrap();
        let conn = db.0.lock().unwrap();
        let fk: i64 = conn
            .query_row("PRAGMA foreign_keys", [], |r| r.get(0))
            .unwrap();
        assert_eq!(fk, 1, "foreign_keys pragma should be ON");
        conn.execute(
            "INSERT INTO media_item (kind, category, category_path, title, path) VALUES ('game','g','g','t','/p')",
            [],
        )
        .unwrap();
        let id: i64 = conn.last_insert_rowid();
        conn.execute(
            "INSERT INTO watch_state (item_id) VALUES (?1)",
            [id],
        )
        .unwrap();
        conn.execute("DELETE FROM media_item WHERE id = ?1", [id])
            .unwrap();
        let remaining: i64 = conn
            .query_row("SELECT count(*) FROM watch_state WHERE item_id = ?1", [id], |r| r.get(0))
            .unwrap();
        assert_eq!(remaining, 0, "cascade delete should remove watch_state row");
    }
}
