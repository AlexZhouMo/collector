//! TMDB API 客户端（api.tmdb.org + image.tmdb.org），ureq 同步请求。

use crate::error::{AppError, AppResult};
use crate::poster::parse::MediaKind;
use std::time::Duration;

const API_BASE: &str = "https://api.tmdb.org/3"; // 主域名被墙，用官方备用域名
const IMG_BASE: &str = "https://image.tmdb.org/t/p/w500";
const UA: &str = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) collector/1.0";

#[derive(Debug, Clone)]
pub struct TmdbHit {
    pub id: u64,
    pub poster_path: Option<String>,
}

fn agent() -> ureq::Agent {
    ureq::AgentBuilder::new()
        .timeout(Duration::from_secs(10))
        .user_agent(UA)
        .build()
}

/// 搜索电影或剧集，取第一条命中。找不到返回 Ok(None)。
pub fn search(name: &str, kind: MediaKind, api_key: &str) -> AppResult<Option<TmdbHit>> {
    let endpoint = match kind {
        MediaKind::Movie => "search/movie",
        MediaKind::Tv => "search/tv",
    };
    let url = format!(
        "{API_BASE}/{endpoint}?api_key={key}&language=zh-CN&query={q}",
        key = api_key,
        q = urlencoding::encode(name),
    );
    let resp = agent()
        .get(&url)
        .call()
        .map_err(|e| AppError::Other(format!("tmdb search: {e}")))?;
    let body = resp
        .into_string()
        .map_err(|e| AppError::Other(format!("tmdb body: {e}")))?;
    let json: serde_json::Value =
        serde_json::from_str(&body).map_err(|e| AppError::Other(format!("tmdb json: {e}")))?;
    let first = json["results"].as_array().and_then(|a| a.first());
    Ok(first.map(|r| TmdbHit {
        id: r["id"].as_u64().unwrap_or(0),
        poster_path: r["poster_path"].as_str().map(|s| s.to_string()),
    }))
}

/// 取剧集某季的海报路径。无海报/无该季返回 Ok(None)。
pub fn season_poster(tv_id: u64, season: u32, api_key: &str) -> AppResult<Option<String>> {
    let url = format!(
        "{API_BASE}/tv/{tv_id}/season/{season}?api_key={api_key}&language=zh-CN"
    );
    let resp = match agent().get(&url).call() {
        Ok(r) => r,
        Err(_) => return Ok(None),
    };
    let body = resp
        .into_string()
        .map_err(|e| AppError::Other(format!("tmdb season body: {e}")))?;
    let json: serde_json::Value = serde_json::from_str(&body)
        .map_err(|e| AppError::Other(format!("tmdb season json: {e}")))?;
    Ok(json["poster_path"].as_str().map(|s| s.to_string()))
}

/// 从 image.tmdb.org 下载 w500 海报字节。
pub fn download(poster_path: &str) -> AppResult<Vec<u8>> {
    let url = format!("{IMG_BASE}{poster_path}");
    let resp = agent()
        .get(&url)
        .call()
        .map_err(|e| AppError::Other(format!("tmdb download: {e}")))?;
    let mut buf = Vec::new();
    use std::io::Read;
    resp.into_reader()
        .read_to_end(&mut buf)
        .map_err(|e| AppError::Other(format!("read poster: {e}")))?;
    Ok(buf)
}
