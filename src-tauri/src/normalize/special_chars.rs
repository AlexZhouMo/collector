//! 特殊字符 / OCR 英文纠错 / 标点规整。数据驱动 + 少量扫描。
//! 基础版：从 Java filter 可确定推断的规则；异体字表留空可扩展（不还原 Java 损坏映射）。

/// 简单串替换表（顺序敏感）。
const REPLACE_TABLE: &[(&str, &str)] = &[
    ("''", "\""),
    ("--", "…"),
    ("lt'", "It'"),
    ("lsn'", "Isn'"),
    (" l ", " I "),
    (" i ", " I "),
    (",,l ", ",,I "),
    ("}l ", "}I "),
    ("\"l ", "\"I "),
    // 异体字/全角拉丁映射（可扩展）：用户日后往此追加，如 ("Ａ","A")
];

/// 连续 2+ 个点 → …；多空格 → 单空格。手写扫描避免引入 regex 依赖。
fn collapse_dots_and_spaces(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let chars: Vec<char> = s.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == '.' {
            let mut j = i;
            while j < chars.len() && chars[j] == '.' {
                j += 1;
            }
            if j - i >= 2 {
                out.push('…');
            } else {
                out.push('.');
            }
            i = j;
        } else {
            out.push(chars[i]);
            i += 1;
        }
    }
    let mut res = String::with_capacity(out.len());
    let mut prev_space = false;
    for c in out.chars() {
        if c == ' ' {
            if !prev_space {
                res.push(' ');
            }
            prev_space = true;
        } else {
            res.push(c);
            prev_space = false;
        }
    }
    res
}

/// 对一段文本应用特殊字符清洗（表 + 点/空格规整）。
pub fn clean_special(s: &str) -> String {
    let mut t = s.to_string();
    for (from, to) in REPLACE_TABLE {
        t = t.replace(from, to);
    }
    collapse_dots_and_spaces(&t)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn ocr_fixes() {
        assert_eq!(clean_special(" l am here"), " I am here");
        assert_eq!(clean_special("lt's ok"), "It's ok");
    }
    #[test]
    fn punct_regular() {
        assert_eq!(clean_special("wait--"), "wait…");
        assert_eq!(clean_special("a....b"), "a…b");
        assert_eq!(clean_special("x   y"), "x y");
    }
    #[test]
    fn quote_normalize() {
        assert_eq!(clean_special("say ''hi''"), "say \"hi\"");
    }
}
