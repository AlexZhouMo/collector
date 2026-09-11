//! 中文维基百科封面客户端：按名查条目封面 URL(zh.wikipedia REST summary) → 经 weserv 代理下载。
//! upload.wikimedia 常被网络阻断，故下载统一走 images.weserv.nl 公开图片代理。

use crate::error::{AppError, AppResult};

/// 从 zh.wikipedia REST summary 响应提取封面原图 URL：
/// originalimage.source 优先，回退 thumbnail.source。无则 None。
pub fn parse_cover_url(json: &serde_json::Value) -> Option<String> {
    json.get("originalimage")
        .and_then(|v| v.get("source"))
        .and_then(|v| v.as_str())
        .or_else(|| {
            json.get("thumbnail")
                .and_then(|v| v.get("source"))
                .and_then(|v| v.as_str())
        })
        .map(|s| s.to_string())
}

/// 把带子系列后缀的标题切到主名用于回退检索：
/// 先 trim，按 '.' 优先、再按空格切，取第一段；无分隔符返回原串。
/// 例："战国.一统记"→"战国"；"圣斗士星矢.EPISODE.G"→"圣斗士星矢"。
pub fn strip_suffix_for_search(name: &str) -> String {
    let name = name.trim();
    let by_dot = name.split('.').next().unwrap_or(name);
    let by_space = by_dot.split(' ').next().unwrap_or(by_dot);
    by_space.trim().to_string()
}

/// 把 upload.wikimedia 图片 URL 包装成 weserv 代理下载 URL：
/// https://images.weserv.nl/?url=<url 整体百分号编码>
pub fn to_proxy_url(image_url: &str) -> String {
    format!("https://images.weserv.nl/?url={}", urlencoding::encode(image_url))
}

fn agent() -> ureq::Agent {
    ureq::AgentBuilder::new()
        .timeout(std::time::Duration::from_secs(15))
        .build()
}

/// 按名查 zh.wikipedia 条目封面 URL（返回原始 upload.wikimedia URL，未经代理）。
/// 全名查：命中即返回；仅当无结果(Ok(None))且切后缀后与原名不同才用主名再查一次；
/// 网络/HTTP 错误（除 404）直接向上传播，不触发回退。
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

/// 单次查询：GET zh.wikipedia REST summary，取封面 URL。
/// 条目不存在(404) → Ok(None)（未命中，非错误）。
fn search_once(keyword: &str) -> AppResult<Option<String>> {
    let url = format!(
        "https://zh.wikipedia.org/api/rest_v1/page/summary/{}",
        urlencoding::encode(keyword)
    );
    let resp = match agent()
        .get(&url)
        .set("Accept", "application/json")
        .set("User-Agent", "zhoumo/collector")
        .call()
    {
        Ok(r) => r,
        // 404 = 无此条目，视为未命中
        Err(ureq::Error::Status(404, _)) => return Ok(None),
        Err(ureq::Error::Status(code, _)) => {
            return Err(AppError::Other(format!("wiki search: HTTP {code}")));
        }
        Err(e) => return Err(AppError::Other(format!("wiki search: {e}"))),
    };
    let text = resp
        .into_string()
        .map_err(|e| AppError::Other(format!("wiki body: {e}")))?;
    let json: serde_json::Value =
        serde_json::from_str(&text).map_err(|e| AppError::Other(format!("wiki json: {e}")))?;
    Ok(parse_cover_url(&json))
}

/// 经 weserv 代理下载封面字节。
pub fn download(url: &str) -> AppResult<Vec<u8>> {
    let proxied = to_proxy_url(url);
    let resp = agent()
        .get(&proxied)
        .set("User-Agent", "zhoumo/collector")
        .call()
        .map_err(|e| AppError::Other(format!("wiki download: {e}")))?;
    let mut buf = Vec::new();
    std::io::Read::read_to_end(&mut resp.into_reader(), &mut buf)
        .map_err(|e| AppError::Other(format!("wiki read: {e}")))?;
    Ok(buf)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_original_then_thumbnail() {
        let j: serde_json::Value = serde_json::from_str(
            r#"{"originalimage":{"source":"https://u/o.jpg"},"thumbnail":{"source":"https://u/t.jpg"}}"#).unwrap();
        assert_eq!(parse_cover_url(&j).as_deref(), Some("https://u/o.jpg"));
        let j2: serde_json::Value = serde_json::from_str(
            r#"{"thumbnail":{"source":"https://u/t.jpg"}}"#).unwrap();
        assert_eq!(parse_cover_url(&j2).as_deref(), Some("https://u/t.jpg"));
        let j3: serde_json::Value = serde_json::from_str(r#"{"title":"x"}"#).unwrap();
        assert_eq!(parse_cover_url(&j3), None);
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

    #[test]
    fn wraps_weserv_proxy() {
        assert_eq!(
            to_proxy_url("https://upload.wikimedia.org/wikipedia/zh/5/54/x.jpg"),
            "https://images.weserv.nl/?url=https%3A%2F%2Fupload.wikimedia.org%2Fwikipedia%2Fzh%2F5%2F54%2Fx.jpg"
        );
    }
}
