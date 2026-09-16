//! TMDB API 客户端（api.tmdb.org + image.tmdb.org），ureq 同步请求。

use crate::error::{AppError, AppResult};
use crate::poster::parse::MediaKind;
use std::time::Duration;

const API_BASE: &str = "https://api.tmdb.org/3"; // 主域名被墙，用官方备用域名
const IMG_BASE: &str = "https://image.tmdb.org/t/p/w500";
const USER_AGENT: &str = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) collector/1.0";

#[derive(Debug, Clone)]
pub struct TmdbHit {
    pub id: u64,
    pub poster_path: Option<String>,
    pub year: Option<u32>,
}

/// 详细命中：用于优化建议，带中文标题与年份。
#[derive(Debug, Clone)]
pub struct TmdbDetail {
    pub title: String,
    pub year: Option<u32>,
}

/// 共用主体：选 endpoint → 拼 year 参数 → 请求 → 解析 → pick_by_year，
/// 返回选中的原始结果 JSON。search / search_detailed 各自 map 成自己的类型。
fn search_raw(
    name: &str,
    kind: MediaKind,
    year: Option<u32>,
    api_key: &str,
) -> AppResult<Option<serde_json::Value>> {
    let endpoint = match kind {
        MediaKind::Movie => "search/movie",
        MediaKind::Tv => "search/tv",
    };
    let year_param = match (year, kind) {
        (Some(y), MediaKind::Movie) => format!("&year={y}"),
        (Some(y), MediaKind::Tv) => format!("&first_air_date_year={y}"),
        (None, _) => String::new(),
    };
    let url = format!(
        "{API_BASE}/{endpoint}?api_key={key}&language=zh-CN&query={q}{year_param}",
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
    Ok(json["results"]
        .as_array()
        .and_then(|a| pick_by_year(a, year))
        .cloned())
}

/// 搜索取第一条有海报的结果，返回其中文标题+年份（供优化建议年份校验）。
pub fn search_detailed(name: &str, kind: MediaKind, year: Option<u32>, api_key: &str) -> AppResult<Option<TmdbDetail>> {
    let first = search_raw(name, kind, year, api_key)?;
    Ok(first.map(|r| {
        let title = r["title"].as_str().or_else(|| r["name"].as_str()).unwrap_or("").to_string();
        let date = r["release_date"].as_str().or_else(|| r["first_air_date"].as_str()).unwrap_or("");
        let year = date.get(0..4).and_then(|y| y.parse::<u32>().ok());
        TmdbDetail { title, year }
    }))
}

/// 从 TMDB 结果里挑选：有年份要求时优先返回年份匹配(±1)且有海报的结果，
/// 否则退回第一条有海报的结果（保持原行为，不改正确条目）。
fn pick_by_year(results: &[serde_json::Value], want: Option<u32>) -> Option<&serde_json::Value> {
    let has_poster = |r: &serde_json::Value| r["poster_path"].is_string();
    let ryear = |r: &serde_json::Value| -> Option<u32> {
        let d = r["release_date"].as_str().or_else(|| r["first_air_date"].as_str()).unwrap_or("");
        d.get(0..4).and_then(|y| y.parse::<u32>().ok())
    };
    if let Some(w) = want {
        // 优先：有海报且年份 ±1
        if let Some(r) = results.iter().find(|r| has_poster(r) && ryear(r).map(|y| (y as i64 - w as i64).abs() <= 1).unwrap_or(false)) {
            return Some(r);
        }
    }
    // 退回：第一条有海报的
    results.iter().find(|r| has_poster(r))
}

fn agent() -> ureq::Agent {
    ureq::AgentBuilder::new()
        .timeout(Duration::from_secs(10))
        .user_agent(USER_AGENT)
        .build()
}

/// 搜索电影或剧集，取第一条命中。找不到返回 Ok(None)。
/// year 存在时作为年份参数提高精度（movie 用 year，tv 用 first_air_date_year）。
pub fn search(name: &str, kind: MediaKind, year: Option<u32>, api_key: &str) -> AppResult<Option<TmdbHit>> {
    let first = search_raw(name, kind, year, api_key)?;
    Ok(first.map(|r| {
        let date = r["release_date"].as_str().or_else(|| r["first_air_date"].as_str()).unwrap_or("");
        TmdbHit {
            id: r["id"].as_u64().unwrap_or(0),
            poster_path: r["poster_path"].as_str().map(|s| s.to_string()),
            year: date.get(0..4).and_then(|y| y.parse::<u32>().ok()),
        }
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
