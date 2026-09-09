//! 海报自动抓取：TMDB 搜索 → 下载 → 统一压缩 → 回填 cover_path。
pub mod parse;
pub mod tmdb;
pub mod image_proc;

use crate::db::Db;
use crate::error::AppResult;
use crate::library::model::MediaItem;
use parse::{parse_query, MediaQuery};
use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize)]
pub struct FailedItem {
    pub category: String,       // 原始分类：电影/动漫/剧集
    pub category_path: String,  // 完整目录（前端去首级显示）
    pub title: String,          // 名称
    pub reason: String,         // 失败原因
    pub suggest_names: Vec<String>, // 推荐改成的名字（0~2 个候选）
    pub suggest_note: String,   // 建议说明文字
}

#[derive(Debug, Clone, Serialize)]
pub struct FetchReport {
    pub ok: usize,
    pub failed: Vec<FailedItem>,
}

/// 分组 key：
/// - 剧集有季（season=Some(n)）→「剧名|n」，同季合并共用一张。
/// - 电影/动漫/无季条目（season=None）→ 用条目唯一 path，使每条独立成组、各搜各存。
fn group_key(q: &MediaQuery, unique_path: &str) -> String {
    match q.season {
        Some(s) => format!("{}|{}", q.name, s),
        None => format!("path|{}", unique_path),
    }
}

/// 只保留 cover_path 为空的视频。
fn needs_cover(it: &MediaItem) -> bool {
    it.cover_path.as_deref().map(|s| s.trim().is_empty()).unwrap_or(true)
}

/// 编排：对所有空封面视频按剧+季分组，每组抓一次封面，组内回填同一 cover_path。
/// `fetch_cover` 返回处理好的封面绝对路径（已下载+压缩+存盘），失败返回 Err(reason)。
/// `suggest(q, reason)` 对失败组生成 (候选名列表 0~2 个, 说明文字)，每组算一次，组内共用。
/// `progress(done, total, title)` 每处理完一个视频回调一次。
pub fn fetch_posters<FC, SG, PG>(
    db: &Db,
    items: &[MediaItem],
    mut fetch_cover: FC,
    mut suggest: SG,
    mut progress: PG,
) -> AppResult<FetchReport>
where
    FC: FnMut(&MediaQuery) -> Result<String, String>,
    SG: FnMut(&MediaQuery, &str) -> (Vec<String>, String),
    PG: FnMut(usize, usize, &str),
{
    let targets: Vec<&MediaItem> = items.iter().filter(|i| needs_cover(i)).collect();
    let total = targets.len();

    let mut groups: BTreeMap<String, (MediaQuery, Vec<&MediaItem>)> = BTreeMap::new();
    for it in &targets {
        let q = parse_query(&it.category, &it.category_path, &it.title);
        groups.entry(group_key(&q, &it.path)).or_insert_with(|| (q.clone(), Vec::new())).1.push(it);
    }

    let mut ok = 0usize;
    let mut failed = Vec::new();
    let mut done = 0usize;

    for (_key, (q, members)) in groups {
        match fetch_cover(&q) {
            Ok(cover_path) => {
                for it in &members {
                    update_cover_path(db, it.id, &cover_path)?;
                    ok += 1;
                    done += 1;
                    progress(done, total, &it.title);
                }
            }
            Err(reason) => {
                let (names, note) = suggest(&q, &reason); // 每失败组算一次建议，组内共用
                for it in &members {
                    failed.push(FailedItem {
                        category: it.category.clone(),
                        category_path: it.category_path.clone(),
                        title: it.title.clone(),
                        reason: reason.clone(),
                        suggest_names: names.clone(),
                        suggest_note: note.clone(),
                    });
                    done += 1;
                    progress(done, total, &it.title);
                }
            }
        }
    }

    Ok(FetchReport { ok, failed })
}

/// 回填单个视频的 cover_path。
fn update_cover_path(db: &Db, id: i64, cover_path: &str) -> AppResult<()> {
    let conn = db.0.lock().unwrap();
    conn.execute(
        "UPDATE media_item SET cover_path=?1 WHERE id=?2",
        rusqlite::params![cover_path, id],
    )
    .map_err(|e| crate::error::AppError::Db(e.to_string()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::library::model::MediaKind as LibKind;

    fn mk(id: i64, cat: &str, cpath: &str, title: &str, cover: Option<&str>) -> MediaItem {
        MediaItem {
            id,
            kind: LibKind::Video,
            category: cat.into(),
            category_path: cpath.into(),
            title: title.into(),
            path: format!("/p/{id}"),
            subtitle_path: None,
            cover_path: cover.map(|s| s.to_string()),
            description: None,
            platform_ok: true,
            exec_path: None,
        }
    }

    #[test]
    fn groups_same_show_same_season() {
        let db = Db::open_in_memory().unwrap();
        {
            let conn = db.0.lock().unwrap();
            for i in 1..=5i64 {
                conn.execute(
                    "INSERT INTO media_item (id,kind,category,category_path,title,path,scanned_at) VALUES (?1,'video','剧集','x','t',?2,0)",
                    rusqlite::params![i, format!("/p/{i}")],
                ).unwrap();
            }
        }
        let items = vec![
            mk(1, "剧集", "剧集/美剧/黑镜/第1季", "黑镜S01E01", None),
            mk(2, "剧集", "剧集/美剧/黑镜/第1季", "黑镜S01E02", None),
            mk(3, "剧集", "剧集/美剧/黑镜/第1季", "黑镜S01E03", None),
            mk(4, "剧集", "剧集/美剧/黑镜/第2季", "黑镜S02E01", None),
            mk(5, "剧集", "剧集/美剧/黑镜/第2季", "黑镜S02E02", None),
        ];
        let mut calls = 0;
        let report = fetch_posters(
            &db,
            &items,
            |_q| { calls += 1; Ok(format!("/covers/c{calls}.jpg")) },
            |_q, _r| (vec![], String::new()),
            |_d, _t, _title| {},
        ).unwrap();
        assert_eq!(calls, 2, "两组（第1季/第2季）只应各抓一次");
        assert_eq!(report.ok, 5, "5 个视频全部回填");
        assert!(report.failed.is_empty());
    }

    #[test]
    fn movies_same_name_independent_and_skip_existing() {
        let db = Db::open_in_memory().unwrap();
        {
            let conn = db.0.lock().unwrap();
            for i in 1..=3i64 {
                conn.execute(
                    "INSERT INTO media_item (id,kind,category,category_path,title,path,scanned_at) VALUES (?1,'video','电影','x','t',?2,0)",
                    rusqlite::params![i, format!("/p/{i}")],
                ).unwrap();
            }
        }
        let items = vec![
            mk(1, "电影", "电影/科幻/沙丘", "沙丘", None),
            mk(2, "电影", "电影/科幻/沙丘", "沙丘", None), // 同名，但 path 不同 → 独立成组
            mk(3, "电影", "电影/科幻/降临", "降临", Some("/covers/existing.jpg")), // 已有封面→跳过
        ];
        let mut calls = 0;
        let report = fetch_posters(
            &db, &items,
            |_q| { calls += 1; Ok(format!("/covers/dune{calls}.jpg")) },
            |_q, _r| (vec![], String::new()),
            |_d, _t, _title| {},
        ).unwrap();
        assert_eq!(calls, 2, "两个同名沙丘各自独立成组，各搜一次");
        assert_eq!(report.ok, 2, "沙丘两个视频各自回填，降临已有封面被跳过");
        let conn = db.0.lock().unwrap();
        let c1: String = conn.query_row("SELECT cover_path FROM media_item WHERE id=1", [], |r| r.get(0)).unwrap();
        let c2: String = conn.query_row("SELECT cover_path FROM media_item WHERE id=2", [], |r| r.get(0)).unwrap();
        assert_ne!(c1, c2, "同名电影各自独立封面，不应相同");
    }

    #[test]
    fn failed_group_recorded_not_filled() {
        let db = Db::open_in_memory().unwrap();
        {
            let conn = db.0.lock().unwrap();
            conn.execute("INSERT INTO media_item (id,kind,category,category_path,title,path,scanned_at) VALUES (1,'video','电影','x','冷门片','/p/1',0)", []).unwrap();
        }
        let items = vec![mk(1, "电影", "电影/冷门片", "冷门片", None)];
        let report = fetch_posters(
            &db, &items,
            |_q| Err("搜索无结果".to_string()),
            |_q, _r| (vec![], "建议手动查证".to_string()),
            |_d, _t, _title| {},
        ).unwrap();
        assert_eq!(report.ok, 0);
        assert_eq!(report.failed.len(), 1);
        assert_eq!(report.failed[0].reason, "搜索无结果");
    }
}
