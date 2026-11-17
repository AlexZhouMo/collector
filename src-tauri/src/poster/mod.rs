//! 海报自动抓取：TMDB 搜索 → 下载 → 统一压缩 → 存盘。
//!
//! 由影视编辑框的「自动抓取」按单条触发（见 lib.rs `fetch_poster_for`）：只返回新封面路径，
//! 不直接改库，和「选择图片」一样等用户点保存时才落库。
pub mod parse;
pub mod tmdb;
pub mod image_proc;

/// 把抓取失败原因与改名建议拼成一句给用户看的提示。
pub fn failure_message(reason: &str, suggest_name: Option<&str>, suggest_note: &str) -> String {
    match suggest_name {
        Some(name) => format!("抓取失败（{reason}）。建议改名为「{name}」：{suggest_note}"),
        None => format!("抓取失败（{reason}）。{suggest_note}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn failure_message_with_and_without_suggestion() {
        assert_eq!(
            failure_message("搜索无结果", Some("星球大战"), "译名/名称与 TMDB 不符，改为此名可命中"),
            "抓取失败（搜索无结果）。建议改名为「星球大战」：译名/名称与 TMDB 不符，改为此名可命中"
        );
        assert_eq!(
            failure_message("网络错误: timeout", None, "未找到可靠候选"),
            "抓取失败（网络错误: timeout）。未找到可靠候选"
        );
    }
}
