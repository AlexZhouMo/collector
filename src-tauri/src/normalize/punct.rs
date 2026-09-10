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
