//! 中英标点体系。中文段全角化 + 「」配对；英文段半角规整（英文函数在后续任务加）。

/// 中文段标点体系：半角标点→全角；成对 " → 「」（奇偶切换，奇数开「偶数闭」）。
pub fn cn_punct(s: &str) -> String {
    let mut t = s.to_string();
    t = t.replace(", ", "，").replace(',', "，");
    t = t.replace("! ", "！").replace('!', "！");
    t = t.replace("? ", "？").replace('?', "？");
    t = t.replace(": ", "：").replace(':', "：");
    t = t.replace("...", "…");
    t = t.replace(". ", "。");
    // 引号配对：第 1、3、5… 个 " → 「，第 2、4… 个 → 」
    let mut res = String::with_capacity(t.len());
    let mut open = true;
    for c in t.chars() {
        if c == '"' {
            res.push(if open { '「' } else { '」' });
            open = !open;
        } else {
            res.push(c);
        }
    }
    res
}

/// 英文段标点体系：半角规整（标点前空格移除、逗号后补空格）、行尾字母/数字补句号。
pub fn en_punct(s: &str) -> String {
    let mut t = s.trim().to_string();
    // 标点前的空格移除
    t = t.replace(" ,", ",").replace(" .", ".").replace(" !", "!").replace(" ?", "?");
    // 逗号后补空格，再压多空格
    t = t.replace(",", ", ").replace("  ", " ");
    // 句点后若紧跟非空白由后续处理；这里规整 ". \"" → ".\""（引号顺序）
    t = t.replace(". \"", ".\"");
    let t = t.trim().to_string();
    // 行尾字母/数字 → 补英文句号
    if t.chars().last().map(|c| c.is_ascii_alphanumeric()).unwrap_or(false) {
        format!("{t}.")
    } else {
        t
    }
}

#[cfg(test)]
mod cn_tests {
    use super::*;
    #[test]
    fn cn_punct_fullwidth() {
        assert_eq!(cn_punct("你好, 世界! 是吗? 好: 嗯"), "你好，世界！是吗？好：嗯");
    }
    #[test]
    fn cn_quotes_paired() {
        assert_eq!(cn_punct("他说\"你好\""), "他说「你好」");
        assert_eq!(cn_punct("\"引"), "「引");
    }
}

#[cfg(test)]
mod en_tests {
    use super::*;
    #[test]
    fn en_punct_regular() {
        assert_eq!(en_punct("Hello ,world"), "Hello, world."); // world 结尾补句号
        assert_eq!(en_punct("Wait . Go"), "Wait. Go.");        // Go 结尾补句号
        assert_eq!(en_punct("end"), "end.");
    }
    #[test]
    fn en_keeps_ellipsis() {
        assert_eq!(en_punct("well…"), "well…");
    }
}
