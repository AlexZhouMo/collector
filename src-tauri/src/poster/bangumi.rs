//! Bangumi(bgm.tv) v0 客户端：按名搜书籍(漫画)封面 → 下载。无需 API key。

use crate::error::{AppError, AppResult};

/// 从 Bangumi v0 搜索响应提取封面 URL：data[0].images.large，回退 common。
pub fn parse_cover_url(json: &serde_json::Value) -> Option<String> {
    let imgs = json.get("data")?.get(0)?.get("images")?;
    imgs.get("large")
        .and_then(|v| v.as_str())
        .or_else(|| imgs.get("common").and_then(|v| v.as_str()))
        .map(|s| s.to_string())
}

/// 把带子系列后缀的标题切到主名用于回退检索：
/// 按 '.' 优先、再按空格切，取第一段；无分隔符返回原串。
/// 例："战国.一统记"→"战国"；"圣斗士星矢.EPISODE.G"→"圣斗士星矢"。
pub fn strip_suffix_for_search(name: &str) -> String {
    let name = name.trim();
    let by_dot = name.split('.').next().unwrap_or(name);
    let by_space = by_dot.split(' ').next().unwrap_or(by_dot);
    by_space.trim().to_string()
}

fn agent() -> ureq::Agent {
    ureq::AgentBuilder::new()
        .timeout(std::time::Duration::from_secs(15))
        .build()
}

/// 按名搜 Bangumi 书籍(type=1)封面 URL。无需 API key。
/// 全名搜：命中即返回；仅当结果为空(Ok(None))且切后缀后与原名不同才用主名再搜一次；
/// 网络/HTTP 错误直接向上传播，不触发回退。
pub fn search_cover(name: &str) -> AppResult<Option<String>> {
    if let Some(url) = search_once(name)? {
        return Ok(Some(url));
    }
    let stripped = strip_suffix_for_search(name);
    if stripped != name && !stripped.is_empty() {
        return search_once(&stripped);
    }
    Ok(None)
}

/// 单次搜索：POST v0/search/subjects，返回首条封面 URL（无结果 → None）。
fn search_once(keyword: &str) -> AppResult<Option<String>> {
    let body = serde_json::json!({ "keyword": keyword, "filter": { "type": [1] } });
    let body_str = body.to_string();
    let resp = match agent()
        .post("https://api.bgm.tv/v0/search/subjects?limit=5")
        .set("Content-Type", "application/json")
        .set("Accept", "application/json")
        .set("User-Agent", "zhoumo/collector")
        .send_string(&body_str)
    {
        Ok(r) => r,
        Err(ureq::Error::Status(code, _resp)) => {
            return Err(AppError::Other(format!("bangumi search: HTTP {code}")));
        }
        Err(e) => return Err(AppError::Other(format!("bangumi search: {e}"))),
    };
    let text = resp
        .into_string()
        .map_err(|e| AppError::Other(format!("bangumi body: {e}")))?;
    let json: serde_json::Value =
        serde_json::from_str(&text).map_err(|e| AppError::Other(format!("bangumi json: {e}")))?;
    Ok(parse_cover_url(&json))
}

/// 下载封面字节。
pub fn download(url: &str) -> AppResult<Vec<u8>> {
    let resp = agent()
        .get(url)
        .set("User-Agent", "zhoumo/collector")
        .call()
        .map_err(|e| AppError::Other(format!("bangumi download: {e}")))?;
    let mut buf = Vec::new();
    std::io::Read::read_to_end(&mut resp.into_reader(), &mut buf)
        .map_err(|e| AppError::Other(format!("bangumi read: {e}")))?;
    Ok(buf)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_large_then_common() {
        let j: serde_json::Value = serde_json::from_str(
            r#"{"data":[{"images":{"large":"https://x/l.jpg","common":"https://x/c.jpg"}}]}"#).unwrap();
        assert_eq!(parse_cover_url(&j).as_deref(), Some("https://x/l.jpg"));
        let j2: serde_json::Value = serde_json::from_str(
            r#"{"data":[{"images":{"common":"https://x/c.jpg"}}]}"#).unwrap();
        assert_eq!(parse_cover_url(&j2).as_deref(), Some("https://x/c.jpg"));
        let j3: serde_json::Value = serde_json::from_str(r#"{"data":[]}"#).unwrap();
        assert_eq!(parse_cover_url(&j3), None);
        let j4: serde_json::Value = serde_json::from_str(r#"{"data":[{"name":"x"}]}"#).unwrap();
        assert_eq!(parse_cover_url(&j4), None);
    }

    #[test]
    fn strips_series_suffix() {
        assert_eq!(strip_suffix_for_search("战国.一统记"), "战国");
        assert_eq!(strip_suffix_for_search("圣斗士星矢.EPISODE.G"), "圣斗士星矢");
        assert_eq!(strip_suffix_for_search("海贼王"), "海贼王");
        assert_eq!(strip_suffix_for_search("one piece manga"), "one");
        assert_eq!(strip_suffix_for_search(" 战国.记"), "战国");
        assert_eq!(strip_suffix_for_search(""), "");
    }
}
