use crate::db::Db;
use crate::error::{AppError, AppResult};
use rusqlite::params;

pub fn set(db: &Db, key: &str, value: &str) -> AppResult<()> {
    let conn = db.0.lock().unwrap();
    conn.execute(
        "INSERT INTO settings(key,value) VALUES(?1,?2)
         ON CONFLICT(key) DO UPDATE SET value=excluded.value",
        params![key, value],
    )
    .map_err(|e| AppError::Db(e.to_string()))?;
    Ok(())
}

pub fn get(db: &Db, key: &str) -> AppResult<Option<String>> {
    let conn = db.0.lock().unwrap();
    conn.query_row(
        "SELECT value FROM settings WHERE key=?1",
        params![key],
        |r| r.get(0),
    )
    .map(Some)
    .or_else(|e| match e {
        rusqlite::Error::QueryReturnedNoRows => Ok(None),
        other => Err(AppError::Db(other.to_string())),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn set_get_roundtrip() {
        let db = Db::open_in_memory().unwrap();
        assert_eq!(get(&db, "video_root").unwrap(), None);
        set(&db, "video_root", "/媒体/视频").unwrap();
        assert_eq!(
            get(&db, "video_root").unwrap().as_deref(),
            Some("/媒体/视频")
        );
    }
}
