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
}
