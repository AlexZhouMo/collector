//! 字幕样式分类与随分类而来的文字规整。
//!
//! 四种样式，按顺序命中即止：
//! 有英文译文 → Bilingual；整行一对《…》→ Title；整行一对 (…) / […] → Note；其余单行 → Unilingual。
//! 「完整包裹」= 开头的括号与结尾的括号是同一对（同族嵌套不影响，多段拼接不算）。
//! 判断一律基于去掉 `{...}` 标签后的可见文字；规整时不碰标签内容。

use crate::normalize::subtitle::strip_override_tags;

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum Style { Bilingual, Unilingual, Title, Note }

impl Style {
    pub fn name(&self) -> &'static str {
        match self {
            Style::Bilingual => "Bilingual",
            Style::Unilingual => "Unilingual",
            Style::Title => "Title",
            Style::Note => "Note",
        }
    }
}

/// 归类时拿不准、需要人工判断的情形（不自动改，只在结果列表提示）。
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum Doubt {
    /// 双语行的中文段被《》或括号完整包裹：按双语归类，但它也可能是标题/注释。
    BilingualWrapped,
    /// 单行由多段包裹拼成：`(枪声)(枪声)`。
    MultiSegment,
    /// 以圆括号开头却没有完整包裹，或开头的括号/书名号整行没有闭合。
    Unclosed,
}

impl Doubt {
    pub fn label(&self) -> &'static str {
        match self {
            Doubt::BilingualWrapped => "样式存疑(双语)",
            Doubt::MultiSegment => "样式存疑(多段)",
            Doubt::Unclosed => "样式存疑(未闭合)",
        }
    }
}

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub struct Classified {
    pub style: Style,
    pub doubt: Option<Doubt>,
}

/// 括号族：同族内半角、全角、实心写法等价。
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
enum Family { Round, Square, Title }

fn opener(c: char) -> Option<Family> {
    match c {
        '(' | '（' => Some(Family::Round),
        '[' | '［' | '【' => Some(Family::Square),
        '《' => Some(Family::Title),
        _ => None,
    }
}

fn closer(c: char) -> Option<Family> {
    match c {
        ')' | '）' => Some(Family::Round),
        ']' | '］' | '】' => Some(Family::Square),
        '》' => Some(Family::Title),
        _ => None,
    }
}

/// 整行可见文字的包裹形态。
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
enum Shape {
    /// 不以括号/书名号开头。
    Plain,
    /// 整行恰好是一对。
    Whole(Family),
    /// 整行由两段及以上包裹拼成（段间只有空白）。
    Multi,
    /// 开头的那一对整行没有闭合。
    Unclosed(Family),
    /// 开头那一对闭合了，但后面还有别的文字。
    Trailing(Family),
}

/// 从 `start`（一个 `fam` 族的开括号）起，按同族计深度，返回与之配对的闭括号下标。
fn segment_end(chars: &[char], start: usize, fam: Family) -> Option<usize> {
    let mut depth = 0usize;
    for (i, &c) in chars.iter().enumerate().skip(start) {
        if opener(c) == Some(fam) {
            depth += 1;
        } else if closer(c) == Some(fam) {
            depth -= 1;
            if depth == 0 {
                return Some(i);
            }
        }
    }
    None
}

fn shape(visible: &str) -> Shape {
    let chars: Vec<char> = visible.trim().chars().collect();
    let Some(first) = chars.first().copied().and_then(opener) else {
        return Shape::Plain;
    };
    let mut i = 0;
    let mut segments = 0;
    loop {
        while i < chars.len() && chars[i].is_whitespace() {
            i += 1;
        }
        if i == chars.len() {
            break;
        }
        let Some(fam) = opener(chars[i]) else {
            return Shape::Trailing(first);
        };
        match segment_end(&chars, i, fam) {
            Some(end) => {
                segments += 1;
                i = end + 1;
            }
            None if segments == 0 => return Shape::Unclosed(first),
            None => return Shape::Trailing(first),
        }
    }
    if segments == 1 { Shape::Whole(first) } else { Shape::Multi }
}

/// 按中文段（可带 `{...}` 标签）和有无英文译文归类。
pub fn classify(zh: &str, has_en: bool) -> Classified {
    let sh = shape(&strip_override_tags(zh));
    if has_en {
        let doubt = matches!(sh, Shape::Whole(_)).then_some(Doubt::BilingualWrapped);
        return Classified { style: Style::Bilingual, doubt };
    }
    let (style, doubt) = match sh {
        Shape::Whole(Family::Title) => (Style::Title, None),
        Shape::Whole(_) => (Style::Note, None),
        Shape::Multi => (Style::Unilingual, Some(Doubt::MultiSegment)),
        Shape::Unclosed(_) | Shape::Trailing(Family::Round) => (Style::Unilingual, Some(Doubt::Unclosed)),
        // `[外语] 早上好`、`《某书》读后感`：标签/书名在前、正文在后，属正常写法
        Shape::Trailing(_) | Shape::Plain => (Style::Unilingual, None),
    };
    Classified { style, doubt }
}

/// 逐字符改写 `{...}` 标签之外的文字；`f` 返回 None 表示删掉该字符。标签内容原样保留。
fn map_outside_tags(s: &str, mut f: impl FnMut(char) -> Option<char>) -> String {
    let mut out = String::with_capacity(s.len());
    let mut in_tag = false;
    for c in s.chars() {
        if in_tag {
            out.push(c);
            if c == '}' {
                in_tag = false;
            }
        } else if c == '{' {
            in_tag = true;
            out.push(c);
        } else if let Some(m) = f(c) {
            out.push(m);
        }
    }
    out
}

/// 删除紧贴在 `《` 前面的空白（半角/全角空格、制表符）：`是 《奇异恩典》啊` → `是《奇异恩典》啊`。
pub fn strip_space_before_title_mark(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut pending = String::new();
    for c in s.chars() {
        if c == ' ' || c == '\u{3000}' || c == '\t' {
            pending.push(c);
            continue;
        }
        if c != '《' {
            out.push_str(&pending);
        }
        pending.clear();
        out.push(c);
    }
    out.push_str(&pending);
    out
}

/// Title 行规整：书名号内冒号统一为中文冒号，删除全部空白。
pub fn normalize_title(s: &str) -> String {
    map_outside_tags(s, |c| match c {
        ':' | '∶' => Some('：'),
        c if c.is_whitespace() => None,
        c => Some(c),
    })
}

/// Note 行规整：中文括号统一为英文括号（行内嵌套的同样转换）。
pub fn normalize_note(s: &str) -> String {
    map_outside_tags(s, |c| Some(match c {
        '（' => '(',
        '）' => ')',
        '【' | '［' => '[',
        '】' | '］' => ']',
        c => c,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn check(cases: &[&str], has_en: bool, style: Style, doubt: Option<Doubt>) {
        for z in cases {
            assert_eq!(classify(z, has_en), Classified { style, doubt }, "{z:?} has_en={has_en}");
        }
    }

    #[test]
    fn whole_book_title_is_title() {
        check(&["《肖申克的救赎》", "{\\an8}《X》", "  《X》  ", "《》"], false, Style::Title, None);
    }

    #[test]
    fn partial_book_title_and_plain_text_are_unilingual() {
        check(&["《X》后面", "前面《X》", "看过《圣经》", "普通台词", ""], false, Style::Unilingual, None);
    }

    #[test]
    fn whole_bracket_is_note() {
        check(
            &["（一把钥匙）", "(a)", "[第1回]", "【突发新闻】", "［咒语］", "（X)", "【X]",
              "(电话(626) 584)", "(正在播放 《某歌》)", "{\\blur3}[咒语]"],
            false, Style::Note, None);
    }

    #[test]
    fn leading_tag_then_text_is_unilingual_without_doubt() {
        check(&["[外语] 早上好", "【警告】请勿靠近", "- [外语] 你好 - [外语] 再见"], false, Style::Unilingual, None);
    }

    #[test]
    fn multiple_wrapped_segments_are_doubtful() {
        check(&["(枪声)(枪声)", "（a） （b）", "《A》 《B》", "(a)[b]"],
            false, Style::Unilingual, Some(Doubt::MultiSegment));
    }

    #[test]
    fn unclosed_or_round_bracket_with_trailing_text_is_doubtful() {
        check(&["（两天没换的衬衫", "（贝克街） 221号B", "(", "[未闭合", "《未闭合", "(a)\\N(b)"],
            false, Style::Unilingual, Some(Doubt::Unclosed));
    }

    #[test]
    fn any_row_with_english_is_bilingual() {
        check(&["你好", "[外语] 早上好", "看过《圣经》", "(枪声)(枪声)", "（两天没换的衬衫"],
            true, Style::Bilingual, None);
    }

    #[test]
    fn wrapped_chinese_with_english_is_bilingual_but_doubtful() {
        check(&["《小红帽》", "（追踪咒）", "[咒语]", "{\\be1}【砰】"],
            true, Style::Bilingual, Some(Doubt::BilingualWrapped));
    }

    #[test]
    fn stray_closing_bracket_does_not_panic() {
        check(&["）开头是闭括号", "》", "]]"], false, Style::Unilingual, None);
    }

    #[test]
    fn style_and_doubt_names() {
        assert_eq!(
            [Style::Bilingual.name(), Style::Unilingual.name(), Style::Title.name(), Style::Note.name()],
            ["Bilingual", "Unilingual", "Title", "Note"]);
        assert_eq!(
            [Doubt::BilingualWrapped.label(), Doubt::MultiSegment.label(), Doubt::Unclosed.label()],
            ["样式存疑(双语)", "样式存疑(多段)", "样式存疑(未闭合)"]);
    }

    #[test]
    fn space_before_title_mark_removed() {
        assert_eq!(strip_space_before_title_mark("是 《奇异恩典》啊"), "是《奇异恩典》啊");
        assert_eq!(strip_space_before_title_mark("《A》 　《B》"), "《A》《B》");
        assert_eq!(strip_space_before_title_mark(" 《A》"), "《A》");
        assert_eq!(strip_space_before_title_mark("a b "), "a b ");
    }

    #[test]
    fn title_colon_and_spaces_normalized() {
        assert_eq!(normalize_title("《斯巴达克斯: 血与沙》"), "《斯巴达克斯：血与沙》");
        assert_eq!(normalize_title("《圣斗士星矢　天界篇 序章》"), "《圣斗士星矢天界篇序章》");
        assert_eq!(normalize_title("{\\fnMicrosoft YaHei}《A B》"), "{\\fnMicrosoft YaHei}《AB》");
    }

    #[test]
    fn note_brackets_become_ascii() {
        assert_eq!(normalize_note("（电话（626）584）"), "(电话(626)584)");
        assert_eq!(normalize_note("【砰】"), "[砰]");
        assert_eq!(normalize_note("{\\pos（1，2）}［X］"), "{\\pos（1，2）}[X]");
    }
}
