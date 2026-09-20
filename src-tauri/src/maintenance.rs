use crate::db::Db;
use crate::error::{AppError, AppResult};
use serde::Serialize;

struct Row {
    category: String,
    category_path: String,
    title: String,
    description: Option<String>,
    cover_path: Option<String>,
}

fn category_rank(c: &str) -> u8 {
    match c {
        "电影" => 0,
        "动漫" => 1,
        "剧集" => 2,
        _ => 3,
    }
}

fn sort_rows(rows: &mut [Row]) {
    rows.sort_by(|a, b| {
        category_rank(&a.category)
            .cmp(&category_rank(&b.category))
            .then_with(|| a.category_path.cmp(&b.category_path))
            .then_with(|| a.title.cmp(&b.title))
    });
}

#[derive(Serialize)]
pub struct DbResetResult {
    pub media: usize,
    pub comic: usize,
    pub game: usize,
}

/// 三表在一个事务内：读出→排序→清空+清 sqlite_sequence→按序重插（ID 从 1）。失败回滚。
pub fn db_reset_impl(db: &Db) -> AppResult<DbResetResult> {
    let mut conn = db.0.lock().unwrap();
    let tx = conn
        .transaction()
        .map_err(|e| AppError::Db(e.to_string()))?;
    let mut counts = [0usize; 3];
    for (i, table) in ["media", "comic", "game"].iter().enumerate() {
        let mut rows: Vec<Row> = {
            let mut stmt = tx
                .prepare(&format!(
                    "SELECT category,category_path,title,description,cover_path FROM {table}"
                ))
                .map_err(|e| AppError::Db(e.to_string()))?;
            let it = stmt
                .query_map([], |r| {
                    Ok(Row {
                        category: r.get(0)?,
                        category_path: r.get(1)?,
                        title: r.get(2)?,
                        description: r.get(3)?,
                        cover_path: r.get(4)?,
                    })
                })
                .map_err(|e| AppError::Db(e.to_string()))?;
            it.collect::<Result<Vec<_>, _>>()
                .map_err(|e| AppError::Db(e.to_string()))?
        };
        sort_rows(&mut rows);
        tx.execute(&format!("DELETE FROM {table}"), [])
            .map_err(|e| AppError::Db(e.to_string()))?;
        tx.execute("DELETE FROM sqlite_sequence WHERE name=?1", [table])
            .map_err(|e| AppError::Db(e.to_string()))?;
        for row in &rows {
            tx.execute(
                &format!(
                    "INSERT INTO {table} (category,category_path,title,description,cover_path) VALUES (?1,?2,?3,?4,?5)"
                ),
                rusqlite::params![
                    row.category,
                    row.category_path,
                    row.title,
                    row.description,
                    row.cover_path
                ],
            )
            .map_err(|e| AppError::Db(e.to_string()))?;
        }
        counts[i] = rows.len();
    }
    tx.commit().map_err(|e| AppError::Db(e.to_string()))?;
    Ok(DbResetResult {
        media: counts[0],
        comic: counts[1],
        game: counts[2],
    })
}

#[tauri::command]
pub fn db_reset(db: tauri::State<Db>) -> AppResult<DbResetResult> {
    db_reset_impl(&db)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(category: &str, path: &str, title: &str) -> Row {
        Row {
            category: category.to_string(),
            category_path: path.to_string(),
            title: title.to_string(),
            description: None,
            cover_path: None,
        }
    }

    #[test]
    fn sort_rows_orders_by_category_then_path_then_title() {
        let mut rows = vec![
            row("其他", "z/path", "aaa"),
            row("剧集", "b/path", "beta"),
            row("动漫", "a/path", "zeta"),
            row("动漫", "a/path", "alpha"),
            row("电影", "m/path", "movie2"),
            row("电影", "a/path", "movie1"),
            row("剧集", "b/path", "alpha"),
        ];
        sort_rows(&mut rows);

        // 期望顺序：电影(a/path,movie1) 电影(m/path,movie2) 动漫(a/path,alpha) 动漫(a/path,zeta)
        //          剧集(b/path,alpha) 剧集(b/path,beta) 其他(z/path,aaa)
        let got: Vec<(&str, &str, &str)> = rows
            .iter()
            .map(|r| {
                (
                    r.category.as_str(),
                    r.category_path.as_str(),
                    r.title.as_str(),
                )
            })
            .collect();
        assert_eq!(
            got,
            vec![
                ("电影", "a/path", "movie1"),
                ("电影", "m/path", "movie2"),
                ("动漫", "a/path", "alpha"),
                ("动漫", "a/path", "zeta"),
                ("剧集", "b/path", "alpha"),
                ("剧集", "b/path", "beta"),
                ("其他", "z/path", "aaa"),
            ]
        );
    }

    #[test]
    fn db_reset_impl_reorders_and_resets_ids() {
        let db = Db::open_in_memory().unwrap();
        {
            let conn = db.0.lock().unwrap();
            // media 乱序插入：剧集/动漫/电影
            let media_rows = [
                ("剧集", "series/x", "剧集A"),
                ("动漫", "anime/y", "动漫B"),
                ("电影", "movie/a", "电影C"),
                ("电影", "movie/a", "电影A"),
            ];
            for (cat, path, title) in media_rows {
                conn.execute(
                    "INSERT INTO media (category,category_path,title) VALUES (?1,?2,?3)",
                    rusqlite::params![cat, path, title],
                )
                .unwrap();
            }
            // comic：分类单一，实际按 path→title
            let comic_rows = [
                ("漫画", "comic/b", "漫画2"),
                ("漫画", "comic/a", "漫画1"),
            ];
            for (cat, path, title) in comic_rows {
                conn.execute(
                    "INSERT INTO comic (category,category_path,title) VALUES (?1,?2,?3)",
                    rusqlite::params![cat, path, title],
                )
                .unwrap();
            }
            // game
            let game_rows = [("游戏", "game/z", "游戏1")];
            for (cat, path, title) in game_rows {
                conn.execute(
                    "INSERT INTO game (category,category_path,title) VALUES (?1,?2,?3)",
                    rusqlite::params![cat, path, title],
                )
                .unwrap();
            }
        }

        let result = db_reset_impl(&db).unwrap();
        assert_eq!(result.media, 4);
        assert_eq!(result.comic, 2);
        assert_eq!(result.game, 1);

        let conn = db.0.lock().unwrap();
        // media 排序验证 + id 从 1 连续
        let mut stmt = conn
            .prepare("SELECT id,category,category_path,title FROM media ORDER BY id")
            .unwrap();
        let media: Vec<(i64, String, String, String)> = stmt
            .query_map([], |r| {
                Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?))
            })
            .unwrap()
            .map(|x| x.unwrap())
            .collect();
        let media_view: Vec<(i64, &str, &str, &str)> = media
            .iter()
            .map(|(id, c, p, t)| (*id, c.as_str(), p.as_str(), t.as_str()))
            .collect();
        assert_eq!(
            media_view,
            vec![
                (1, "电影", "movie/a", "电影A"),
                (2, "电影", "movie/a", "电影C"),
                (3, "动漫", "anime/y", "动漫B"),
                (4, "剧集", "series/x", "剧集A"),
            ]
        );

        // comic 按 path→title，id 从 1
        let mut stmt = conn
            .prepare("SELECT id,category_path,title FROM comic ORDER BY id")
            .unwrap();
        let comic: Vec<(i64, String, String)> = stmt
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
            .unwrap()
            .map(|x| x.unwrap())
            .collect();
        let comic_view: Vec<(i64, &str, &str)> = comic
            .iter()
            .map(|(id, p, t)| (*id, p.as_str(), t.as_str()))
            .collect();
        assert_eq!(
            comic_view,
            vec![(1, "comic/a", "漫画1"), (2, "comic/b", "漫画2")]
        );

        // game id 从 1
        let mut stmt = conn.prepare("SELECT id FROM game ORDER BY id").unwrap();
        let game_ids: Vec<i64> = stmt
            .query_map([], |r| r.get(0))
            .unwrap()
            .map(|x| x.unwrap())
            .collect();
        assert_eq!(game_ids, vec![1]);
    }
}
