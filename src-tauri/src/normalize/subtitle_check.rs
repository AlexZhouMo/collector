use crate::normalize::subtitle::{parse_time_cs, Dialogue};
pub use crate::normalize::subtitle::Issue;

/// 时间轴交叉：按 start 排序后，每条与其后 3 条比较区间是否重叠（as<be && bs<ae）。
pub fn check_timeline_cross(dialogues: &[Dialogue]) -> Vec<Issue> {
    let mut idx: Vec<usize> = (0..dialogues.len()).collect();
    idx.sort_by_key(|&i| parse_time_cs(&dialogues[i].start).unwrap_or(0));
    let mut issues = Vec::new();
    for a in 0..idx.len() {
        let (as_, ae) = match (parse_time_cs(&dialogues[idx[a]].start), parse_time_cs(&dialogues[idx[a]].end)) {
            (Some(s), Some(e)) => (s, e),
            _ => continue,
        };
        for b in (a + 1)..(a + 4).min(idx.len()) {
            let (bs, be) = match (parse_time_cs(&dialogues[idx[b]].start), parse_time_cs(&dialogues[idx[b]].end)) {
                (Some(s), Some(e)) => (s, e),
                _ => continue,
            };
            // 排除时间轴完全相同的记录：那是正常的中英对（未合并的双语行），不算交叉
            let identical = as_ == bs && ae == be;
            if !identical && as_ < be && bs < ae {
                issues.push(Issue {
                    line: idx[b] + 1,
                    kind: "时间轴交叉".into(),
                    text: dialogues[idx[b]].text.clone(),
                });
            }
        }
    }
    issues
}

/// 是否含拉丁字母（英文判据）。
fn has_latin(s: &str) -> bool {
    s.chars().any(|c| c.is_ascii_alphabetic())
}

/// 是否含中文（与 subtitle.rs 的 has_cjk 同范围，本文件内私有副本，避免暴露内部函数）。
fn has_cjk(s: &str) -> bool {
    s.chars().any(|c| ('\u{4e00}'..='\u{9fa5}').contains(&c))
}

/// 整行（trim 后）是否被一对注释标记从头到尾包裹：（…）《…》[…]［…］#…#。
/// 用于识别「标题/旁白/外语/歌词」等正常的单语行，不算漏译。
fn is_fully_wrapped(s: &str) -> bool {
    let t = s.trim();
    let chars: Vec<char> = t.chars().collect();
    if chars.len() < 2 { return false; }
    let (first, last) = (chars[0], chars[chars.len() - 1]);
    matches!((first, last),
        ('（', '）') | ('(', ')') | ('《', '》') |
        ('[', ']') | ('［', '］') | ('#', '#'))
}

/// 单语行检测（疑似漏加注释标记）：仅当文件为双语（存在至少一条带英文译文的对白）时生效。
/// 逐条：无译文（en() 为 None）且整行未被注释标记包裹的裸单语行 → 提示。
/// 含中文报「疑似漏译(仅中文)」；不含中文但含拉丁字母报「疑似漏译(仅英文)」；
/// 二者皆无（纯标点/数字）不报。
pub fn check_monolingual(dialogues: &[Dialogue]) -> Vec<Issue> {
    // 双语文件判定：任一条有英文译文段（含拉丁字母）
    let is_bilingual_file = dialogues.iter().any(|d|
        d.en().map(|e| has_latin(&e)).unwrap_or(false));
    if !is_bilingual_file { return Vec::new(); }

    let mut issues = Vec::new();
    for (i, d) in dialogues.iter().enumerate() {
        if d.en().is_some() { continue; }          // 有译文 → 双语行，跳过
        let zh = d.zh();
        if is_fully_wrapped(&zh) { continue; }      // 被注释标记包裹 → 正常
        let kind = if has_cjk(&zh) {
            "疑似漏译(仅中文)"
        } else if has_latin(&zh) {
            "疑似漏译(仅英文)"
        } else {
            continue;                                // 纯标点/数字 → 不报
        };
        issues.push(Issue { line: i + 1, kind: kind.into(), text: d.text.clone() });
    }
    issues
}

#[cfg(test)]
mod cross_tests {
    use super::*;
    use crate::normalize::subtitle::Dialogue;
    fn d(s:&str,e:&str)->Dialogue{Dialogue{start:s.into(),end:e.into(),text:"x".into()}}
    #[test]
    fn detects_overlap_within_window() {
        let ds = vec![
            d("0:00:01.00","0:00:05.00"),
            d("0:00:04.00","0:00:06.00"),
        ];
        let issues = check_timeline_cross(&ds);
        assert!(issues.iter().any(|i| i.kind == "时间轴交叉"));
    }
    #[test]
    fn no_overlap_ok() {
        let ds = vec![ d("0:00:01.00","0:00:02.00"), d("0:00:03.00","0:00:04.00") ];
        assert!(check_timeline_cross(&ds).is_empty());
    }
    #[test]
    fn identical_timeline_not_crossing() {
        // 完全相同 start+end 的一中一英 = 正常中英对，不应报交叉
        let ds = vec![
            d("0:00:01.00","0:00:05.00"), // 中
            d("0:00:01.00","0:00:05.00"), // 英，时间完全一样
        ];
        assert!(check_timeline_cross(&ds).is_empty());
    }
}

#[cfg(test)]
mod mono_tests {
    use super::*;
    use crate::normalize::subtitle::{Dialogue, SEPARATOR};

    fn bi(zh: &str, en: &str) -> Dialogue {
        Dialogue { start: "0:00:01.00".into(), end: "0:00:02.00".into(),
            text: format!("{zh}{SEPARATOR}{en}") }
    }
    fn mono(t: &str) -> Dialogue {
        Dialogue { start: "0:00:01.00".into(), end: "0:00:02.00".into(), text: t.into() }
    }

    #[test]
    fn bare_chinese_line_in_bilingual_file_flagged() {
        let ds = vec![ bi("你好", "Hi"), mono("中国 北京") ];
        let issues = check_monolingual(&ds);
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].kind, "疑似漏译(仅中文)");
        assert_eq!(issues[0].text, "中国 北京");
        assert_eq!(issues[0].line, 2);
    }

    #[test]
    fn bare_english_line_in_bilingual_file_flagged() {
        let ds = vec![ bi("你好", "Hi"), mono("Beijing China") ];
        let issues = check_monolingual(&ds);
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].kind, "疑似漏译(仅英文)");
    }

    #[test]
    fn wrapped_notes_not_flagged() {
        let ds = vec![
            bi("你好", "Hi"),
            mono("（中国 北京）"),
            mono("《复仇者联盟》"),
            mono("[咒语]"),
            mono("［咒语］"),
            mono("#歌词#"),
        ];
        assert!(check_monolingual(&ds).is_empty());
    }

    #[test]
    fn partial_wrap_still_flagged() {
        let ds = vec![ bi("你好", "Hi"), mono("中国（北京）人") ];
        let issues = check_monolingual(&ds);
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].kind, "疑似漏译(仅中文)");
    }

    #[test]
    fn pure_chinese_file_skipped() {
        let ds = vec![ mono("你好"), mono("中国 北京"), mono("再见") ];
        assert!(check_monolingual(&ds).is_empty());
    }

    #[test]
    fn bilingual_rows_not_flagged() {
        let ds = vec![
            bi("你好", "Hi"),
            bi("- 叫车 - 我不叫", "- Get a cab. - I never get cabs."),
        ];
        assert!(check_monolingual(&ds).is_empty());
    }

    #[test]
    fn punct_only_line_not_flagged() {
        let ds = vec![ bi("你好", "Hi"), mono("...123..."), mono("♪♪♪") ];
        assert!(check_monolingual(&ds).is_empty());
    }
}
