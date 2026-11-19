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

/// 删掉看不见、但播放器会画成方框或干扰排版的字符，制表符换成空格：
/// - 控制字符（U+0000–001F、U+007F–009F）：libass 找不到字形，会画成方框；
/// - 零宽/方向控制等格式字符（软连字符 U+00AD、U+200B–200F、U+202A–202E、U+2060–206F、BOM U+FEFF）：
///   多是从网页/播放器复制字幕时带进来的，本身不显示。
pub fn strip_invisible(s: &str) -> String {
    s.chars()
        .filter_map(|c| match c {
            '\t' => Some(' '),
            c if c.is_control() => None,
            '\u{00AD}' | '\u{200B}'..='\u{200F}' | '\u{202A}'..='\u{202E}'
            | '\u{2060}'..='\u{206F}' | '\u{FEFF}' => None,
            c => Some(c),
        })
        .collect()
}

/// GBK 用户自定义区 A1 行（A140–A17E）解码出的私用区字符 U+E4C6–E504，还原成 Big5 同码位的标点。
///
/// 来历：繁体（Big5）字幕被当作 GBK 转码时，汉字能对上，但 Big5 的标点（A1 行）落在 GBK 的
/// 自定义区，于是变成私用区字符——任何字体都没有它的字形。实测语料里就是 `…` `？` `，` 三种。
/// 其它私用区字符没有可靠的对应关系，不动，交给「疑似乱码」提示人工处理。
pub fn restore_big5_punct(s: &str) -> String {
    const FIRST: u32 = 0xE4C6;
    const LAST: u32 = 0xE504; // 对应尾字节 0x40–0x7E
    if !s.chars().any(|c| (FIRST..=LAST).contains(&(c as u32))) {
        return s.to_string();
    }
    s.chars()
        .map(|c| {
            let cp = c as u32;
            if !(FIRST..=LAST).contains(&cp) {
                return c;
            }
            let bytes = [0xA1u8, 0x40 + (cp - FIRST) as u8];
            let (decoded, _, had_err) = encoding_rs::BIG5.decode(&bytes);
            let mut it = decoded.chars();
            match (had_err, it.next(), it.next()) {
                (false, Some(p), None) => p,
                _ => c,
            }
        })
        .collect()
}

/// 只会出现在乱码里的片段：UTF-8 文本被当成 GBK 读出来的典型产物。
const MOJIBAKE_MARKS: &[&str] = &["锟斤拷", "鈥", "锛", "銆"];

/// 文本里是否残留乱码特征：私用区字符、替换符 U+FFFD、白方框 □（OCR/转码丢字留下的占位）、
/// 或 `MOJIBAKE_MARKS` 里的典型乱码片段。
pub fn has_garbled(s: &str) -> bool {
    s.chars().any(|c| matches!(c, '\u{E000}'..='\u{F8FF}' | '\u{FFFD}' | '□'))
        || MOJIBAKE_MARKS.iter().any(|m| s.contains(m))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn strips_control_and_format_chars() {
        assert_eq!(strip_invisible("- Hi. - \u{202D}Sorry."), "- Hi. - Sorry.");
        assert_eq!(strip_invisible("你是什\u{200B}\u{200B}么意思"), "你是什么意思");
        assert_eq!(strip_invisible("那时候\u{7F} 你\u{1E}好\u{200E}\u{206E}\u{FEFF}"), "那时候 你好");
        assert_eq!(strip_invisible("奥马·\t库珀"), "奥马· 库珀");
        // 正常文本、标签、换行标记原样保留
        assert_eq!(strip_invisible("♪ 你好\\N{\\fnArial\\fs30}Hi ♪"), "♪ 你好\\N{\\fnArial\\fs30}Hi ♪");
    }
    #[test]
    fn restores_big5_punct_from_private_use() {
        assert_eq!(restore_big5_punct("问题是\u{E4D1}"), "问题是…");
        assert_eq!(restore_big5_punct("伊藤开司…\u{E4CE}"), "伊藤开司…？");
        assert_eq!(restore_big5_punct("这样一来\u{E4C7}"), "这样一来，");
        // 范围外的私用区字符没有可靠对应，保持原样
        assert_eq!(restore_big5_punct("瞬间接\u{E769}剂"), "瞬间接\u{E769}剂");
        assert_eq!(restore_big5_punct("普通文本"), "普通文本");
    }
    #[test]
    fn detects_garbled_text() {
        assert!(has_garbled("瞬间接\u{E769}剂"));
        assert!(has_garbled("快堵住入□，快点"));
        assert!(has_garbled("what you鈥檝e done"));
        assert!(has_garbled("坏\u{FFFD}字"));
        assert!(!has_garbled("什么 烫烫烫\\N{\\fnArial\\fs30}Sorry? Hot! Hot!"));
        assert!(!has_garbled("♪ 你好，世界… ♪"));
    }
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
