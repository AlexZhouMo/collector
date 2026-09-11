//! AniList GraphQL 客户端：按名搜 MANGA 封面 → 下载。无需 API key。

use crate::error::{AppError, AppResult};

/// 从 AniList GraphQL 响应提取封面 URL：data.Media.coverImage.extraLarge，回退 large。
pub fn parse_cover_url(json: &serde_json::Value) -> Option<String> {
    let ci = json.get("data")?.get("Media")?.get("coverImage")?;
    ci.get("extraLarge")
        .and_then(|v| v.as_str())
        .or_else(|| ci.get("large").and_then(|v| v.as_str()))
        .map(|s| s.to_string())
}

fn agent() -> ureq::Agent {
    ureq::AgentBuilder::new()
        .timeout(std::time::Duration::from_secs(15))
        .build()
}

/// 按名搜 AniList MANGA 封面 URL。无需 API key。
pub fn search_cover(name: &str) -> AppResult<Option<String>> {
    let query = "query($q:String){ Media(search:$q, type:MANGA){ coverImage{ extraLarge large } } }";
    let body = serde_json::json!({ "query": query, "variables": { "q": name } });
    let body_str = body.to_string();
    let resp = agent()
        .post("https://graphql.anilist.co")
        .set("Content-Type", "application/json")
        .set("Accept", "application/json")
        .send_string(&body_str)
        .map_err(|e| AppError::Other(format!("anilist search: {e}")))?;
    let text = resp
        .into_string()
        .map_err(|e| AppError::Other(format!("anilist body: {e}")))?;
    let json: serde_json::Value =
        serde_json::from_str(&text).map_err(|e| AppError::Other(format!("anilist json: {e}")))?;
    Ok(parse_cover_url(&json))
}

/// 下载封面字节。
pub fn download(url: &str) -> AppResult<Vec<u8>> {
    let resp = agent()
        .get(url)
        .call()
        .map_err(|e| AppError::Other(format!("anilist download: {e}")))?;
    let mut buf = Vec::new();
    std::io::Read::read_to_end(&mut resp.into_reader(), &mut buf)
        .map_err(|e| AppError::Other(format!("anilist read: {e}")))?;
    Ok(buf)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn parses_extra_large_then_large() {
        let j: serde_json::Value = serde_json::from_str(
            r#"{"data":{"Media":{"coverImage":{"extraLarge":"https://x/xl.jpg","large":"https://x/l.jpg"}}}}"#).unwrap();
        assert_eq!(parse_cover_url(&j).as_deref(), Some("https://x/xl.jpg"));
        let j2: serde_json::Value = serde_json::from_str(
            r#"{"data":{"Media":{"coverImage":{"large":"https://x/l.jpg"}}}}"#).unwrap();
        assert_eq!(parse_cover_url(&j2).as_deref(), Some("https://x/l.jpg"));
        let j3: serde_json::Value = serde_json::from_str(r#"{"data":{"Media":null}}"#).unwrap();
        assert_eq!(parse_cover_url(&j3), None);
    }
}
