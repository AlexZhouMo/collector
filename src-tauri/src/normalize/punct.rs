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

/// 一个中文/全角标点在英文里的写法。返回 (替换文本, 是否为句读标点)；
/// 句读标点（逗号、句号、问号、叹号、分号、冒号）紧跟字母时要补一个空格。None 表示不是要换的标点。
fn ascii_punct(c: char) -> Option<(&'static str, bool)> {
    Some(match c {
        '，' | '、' => (",", true),
        '。' | '｡' | '．' => (".", true),
        '？' => ("?", true),
        '！' => ("!", true),
        '；' => (";", true),
        '：' => (":", true),
        '…' | '⋯' => ("...", false),
        '“' | '”' | '„' | '‟' | '＂' | '「' | '」' | '『' | '』' | '《' | '》' | '〈' | '〉' => ("\"", false),
        '‘' | '’' | '‚' | '‛' | '＇' => ("'", false),
        '（' => ("(", false),
        '）' => (")", false),
        '【' | '〔' | '［' => ("[", false),
        '】' | '〕' | '］' => ("]", false),
        '～' | '〜' => ("~", false),
        '―' | '─' => ("—", false),
        '\u{3000}' => (" ", false),
        _ => return None,
    })
}

/// 把英文文本里的中文/全角标点全部换成英文标点（`{...}` 标签内不动）：
/// 逗号、句号、问号、叹号、分号、冒号、省略号（→ `...`）、引号、括号、波浪号等；
/// 全角字母数字转半角；中文破折号 `——` 并成一个 `—`。
fn to_ascii_punct(s: &str) -> String {
    let chars: Vec<char> = s.chars().collect();
    let mut out = String::with_capacity(s.len());
    let mut in_tag = false;
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        i += 1;
        if in_tag {
            in_tag = c != '}';
            out.push(c);
            continue;
        }
        if c == '{' {
            in_tag = true;
            out.push(c);
            continue;
        }
        // 中文破折号 ——（两个 em dash）在英文里是一个 —
        if c == '—' {
            while chars.get(i) == Some(&'—') {
                i += 1;
            }
            out.push('—');
            continue;
        }
        // 中文的双省略号 ……（含 ⋯⋯）在英文里只要一个 ...
        if matches!(c, '…' | '⋯') {
            while matches!(chars.get(i), Some('…' | '⋯')) {
                i += 1;
            }
        }
        if let Some((rep, spaced)) = ascii_punct(c) {
            let next = chars.get(i).copied();
            let next_is_letter = next.is_some_and(|n| n.is_ascii_alphanumeric() || is_fullwidth_letter(n));
            // 开引号前紧贴字母或句读标点时补空格：`said:“go` → `said: "go`
            if is_open_quote(c) && out.chars().last().is_some_and(|p| p.is_ascii_alphanumeric() || ",.!?;:".contains(p)) {
                out.push(' ');
            }
            out.push_str(rep);
            let letter_next = next.is_some_and(|n| n.is_ascii_alphabetic() || is_fullwidth_letter(n));
            if (spaced && (letter_next || next.is_some_and(is_open_quote)))
                || (is_close_quote(c) && next_is_letter)
            {
                out.push(' ');
            }
            continue;
        }
        // 其余全角 ASCII（字母、数字、符号）→ 半角
        if ('\u{FF01}'..='\u{FF5E}').contains(&c) {
            out.push(char::from_u32(c as u32 - 0xFEE0).unwrap_or(c));
            continue;
        }
        out.push(c);
    }
    out
}

/// 能分清开闭的双引号类（单引号 ’ 也是撇号，不在此列）。
fn is_open_quote(c: char) -> bool {
    matches!(c, '“' | '„' | '‟' | '「' | '『' | '《' | '〈')
}
fn is_close_quote(c: char) -> bool {
    matches!(c, '”' | '」' | '』' | '》' | '〉')
}

fn is_fullwidth_letter(c: char) -> bool {
    ('Ａ'..='Ｚ').contains(&c) || ('ａ'..='ｚ').contains(&c)
}

/// 逗号后紧跟非 ASCII 数字、非空白、非闭合符号（引号、括号）时补一个空格。
fn space_after_comma(s: &str) -> String {
    let chars: Vec<char> = s.chars().collect();
    let mut out = String::with_capacity(s.len() + 8);
    for (k, &c) in chars.iter().enumerate() {
        out.push(c);
        if c == ',' {
            if let Some(&n) = chars.get(k + 1) {
                if !(n.is_whitespace() || n.is_ascii_digit() || "\"')]}".contains(n)) {
                    out.push(' ');
                }
            }
        }
    }
    out
}

/// 英文标点规整（不补句号）：中文/全角标点换成英文标点，标点前空格移除、逗号后补空格、多空格并一。
/// 纯英文的单语行用它；英文译文段用 `en_punct`（另补行尾句号）。
pub fn en_marks(s: &str) -> String {
    let mut t = to_ascii_punct(s.trim());
    // 标点前的空格移除
    t = t.replace(" ,", ",").replace(" .", ".").replace(" !", "!").replace(" ?", "?")
        .replace(" ;", ";").replace(" :", ":");
    // 逗号后紧跟字母时补空格（`1,000`、`",` 后的闭引号等不补），再压多空格
    t = space_after_comma(&t);
    while t.contains("  ") {
        t = t.replace("  ", " ");
    }
    // 句点后若紧跟非空白由后续处理；这里规整 ". \"" → ".\""（引号顺序）
    t = t.replace(". \"", ".\"");
    t.trim().to_string()
}

/// 英文段标点体系：`en_marks` 规整后，行尾字母/数字补英文句号。
pub fn en_punct(s: &str) -> String {
    let t = en_marks(s);
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
    fn en_ellipsis_becomes_three_dots() {
        assert_eq!(en_punct("well…"), "well...");
        assert_eq!(en_punct("Wait⋯ go"), "Wait... go.");
    }
    #[test]
    fn en_fullwidth_punct_to_ascii() {
        // 逗号、句号、问号、叹号、分号、冒号：转半角，紧跟字母时补一个空格
        assert_eq!(en_punct("Hello，world。How are you？Fine！"), "Hello, world. How are you? Fine!");
        assert_eq!(en_punct("First；second：third"), "First; second: third.");
        // 时间、数字里的冒号后不补空格
        assert_eq!(en_punct("At 10：30"), "At 10:30.");
        // 引号：中文双引号、直角引号、书名号 → "；中文单引号 → '
        assert_eq!(en_punct("He said “go”"), "He said \"go\"");
        assert_eq!(en_punct("「Run」 and 《Jaws》"), "\"Run\" and \"Jaws\"");
        assert_eq!(en_punct("I don’t ‘know’"), "I don't 'know'");
        // 全角括号、方括号、波浪号、全角空格、全角字母数字
        assert_eq!(en_punct("（laughs）【music】～"), "(laughs)[music]~");
        assert_eq!(en_punct("Ｉ\u{3000}ｗｉｎ１"), "I win1.");
        // 顿号 → 逗号；中文破折号 —— → —
        assert_eq!(en_punct("red、blue——green"), "red, blue—green.");
    }
    #[test]
    fn en_double_ellipsis_and_quote_spacing() {
        // 中文的双省略号 …… 在英文里只要一个 ...
        assert_eq!(en_punct("Let’s go……"), "Let's go...");
        // 开引号前、闭引号后紧贴字母时补空格
        assert_eq!(en_punct("He said：“Let’s go……”Right？"), "He said: \"Let's go...\" Right?");
        assert_eq!(en_punct("the word“yes”now"), "the word \"yes\" now.");
        // 引号内侧不加空格，闭引号后跟标点不加空格
        assert_eq!(en_punct("“Hi，” he said。"), "\"Hi,\" he said.");
    }
    #[test]
    fn en_marks_leave_override_tags_alone() {
        assert_eq!(en_marks("{\\fn微软雅黑，x}Hi，there"), "{\\fn微软雅黑，x}Hi, there");
    }
    #[test]
    fn en_marks_has_no_trailing_period() {
        // 纯英文单语行只换标点，不补句号
        assert_eq!(en_marks("Hello，world"), "Hello, world");
    }
}
