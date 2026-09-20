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

/// 双语文件判定阈值：带英文译文的对白占比需 ≥ 此值，且绝对条数 ≥ MIN_BILINGUAL_ROWS，
/// 才认定为双语文件。避免纯中文片里偶发的一两句背景英文歌词（碰巧与中文台词时间轴
/// 相同被 merge_bilingual 合并出英文段）把整个文件误判为双语。
const BILINGUAL_RATIO: f64 = 0.30;
const MIN_BILINGUAL_ROWS: usize = 5;

/// 单语行检测（疑似漏加注释标记）：仅当文件整体为双语时生效。
/// 双语判定：带英文译文段的对白占比 ≥ 30% 且不少于 5 条（纯中文片偶发英文歌词不触发）。
/// 逐条：无译文（en() 为 None）且整行未被注释标记包裹的裸单语行 → 提示。
/// 含中文报「疑似漏译(仅中文)」；不含中文但含拉丁字母报「疑似漏译(仅英文)」；
/// 二者皆无（纯标点/数字）不报。
pub fn check_monolingual(dialogues: &[Dialogue]) -> Vec<Issue> {
    // 双语文件判定：带英文译文段（含拉丁字母）的对白占比与绝对条数均达阈值。
    let bilingual_rows = dialogues.iter().filter(|d|
        d.en().map(|e| has_latin(&e)).unwrap_or(false)).count();
    let total = dialogues.len();
    let is_bilingual_file = bilingual_rows >= MIN_BILINGUAL_ROWS
        && total > 0
        && (bilingual_rows as f64 / total as f64) >= BILINGUAL_RATIO;
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

/// 非标准歌曲符号：库里歌曲字幕以 ∮ 为规范标记，但有少量行用 ♪♫♬♩ 等乐符
/// （常贴句首/句中、不成对，不宜自动替换）。检测出来提示，人工统一改为 ∮。
const NONSTD_SONG_SYMBOLS: &[char] = &['♪', '♫', '♬', '♩', '♭', '♮', '♯'];

/// 检测含非标准歌曲符号（♪♫♬ 等）的行，提示人工统一为 ∮。
/// 只要文本含任一非标准乐符即提示（即便同时也有 ∮，如「∮ 歌词♬ ∮」，那个 ♬ 也该清理）。
pub fn check_song_symbol(dialogues: &[Dialogue]) -> Vec<Issue> {
    let mut issues = Vec::new();
    for (i, d) in dialogues.iter().enumerate() {
        if d.text.chars().any(|c| NONSTD_SONG_SYMBOLS.contains(&c)) {
            issues.push(Issue {
                line: i + 1,
                kind: "非标准歌曲符(建议改∮)".into(),
                text: d.text.clone(),
            });
        }
    }
    issues
}

#[cfg(test)]
mod cross_tests {
    use super::*;
    use crate::normalize::subtitle::Dialogue;
    fn d(s:&str,e:&str)->Dialogue{Dialogue{start:s.into(),end:e.into(),text:"x".into(),src_lines:vec![1]}}
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
            text: format!("{zh}{SEPARATOR}{en}"), src_lines: vec![1] }
    }
    fn mono(t: &str) -> Dialogue {
        Dialogue { start: "0:00:01.00".into(), end: "0:00:02.00".into(), text: t.into(), src_lines: vec![1] }
    }
    /// 生成 n 条双语行，用于让用例满足双语文件阈值（占比 ≥30% 且 ≥5 条）。
    fn bi_rows(n: usize) -> Vec<Dialogue> {
        (0..n).map(|_| bi("你好", "Hi")).collect()
    }

    #[test]
    fn bare_chinese_line_in_bilingual_file_flagged() {
        // 5 条双语 + 1 条裸中文行 → 双语文件，报「疑似漏译(仅中文)」
        let mut ds = bi_rows(5);
        ds.push(mono("中国 北京"));
        let issues = check_monolingual(&ds);
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].kind, "疑似漏译(仅中文)");
        assert_eq!(issues[0].text, "中国 北京");
        assert_eq!(issues[0].line, 6);
    }

    #[test]
    fn bare_english_line_in_bilingual_file_flagged() {
        let mut ds = bi_rows(5);
        ds.push(mono("Beijing China"));
        let issues = check_monolingual(&ds);
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].kind, "疑似漏译(仅英文)");
    }

    #[test]
    fn wrapped_notes_not_flagged() {
        let mut ds = bi_rows(5);
        ds.extend([
            mono("（中国 北京）"),
            mono("《复仇者联盟》"),
            mono("[咒语]"),
            mono("［咒语］"),
            mono("#歌词#"),
        ]);
        assert!(check_monolingual(&ds).is_empty());
    }

    #[test]
    fn partial_wrap_still_flagged() {
        let mut ds = bi_rows(5);
        ds.push(mono("中国（北京）人"));
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
        let mut ds = bi_rows(4);
        ds.push(bi("- 叫车 - 我不叫", "- Get a cab. - I never get cabs."));
        assert!(check_monolingual(&ds).is_empty());
    }

    #[test]
    fn punct_only_line_not_flagged() {
        let mut ds = bi_rows(5);
        ds.extend([ mono("...123..."), mono("♪♪♪") ]);
        assert!(check_monolingual(&ds).is_empty());
    }

    #[test]
    fn occasional_english_lyric_not_treated_as_bilingual() {
        // 回归：《萤火虫之墓》场景——大量纯中文 + 极少数偶发英文歌词行（占比远低于阈值），
        // 不应判为双语文件，因而裸中文行不报漏译。
        let mut ds: Vec<Dialogue> = (0..50).map(|_| mono("纯中文台词")).collect();
        ds.push(bi("就是啊", "MID PLEASURES AND PALACES")); // 时间轴巧合被合并出的伪双语
        ds.push(mono("中国 北京")); // 裸中文行——纯中文片里属正常，不该报
        assert!(check_monolingual(&ds).is_empty(),
            "偶发英文歌词不应把纯中文片判为双语");
    }

    #[test]
    fn below_min_rows_not_bilingual() {
        // 占比达标但绝对条数不足 5 → 不判双语（极短片段防误判）
        let ds = vec![ bi("你好", "Hi"), bi("再见", "Bye"), mono("中国 北京") ];
        assert!(check_monolingual(&ds).is_empty());
    }
}

#[cfg(test)]
mod song_tests {
    use super::*;
    use crate::normalize::subtitle::Dialogue;
    fn line(t: &str) -> Dialogue {
        Dialogue { start: "0:00:01.00".into(), end: "0:00:02.00".into(), text: t.into(), src_lines: vec![1] }
    }

    #[test]
    fn eighth_note_flagged() {
        let ds = vec![ line("-所以我  -♪耶  今天是我的生日") ];
        let issues = check_song_symbol(&ds);
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].kind, "非标准歌曲符(建议改∮)");
        assert_eq!(issues[0].line, 1);
    }

    #[test]
    fn other_music_notes_flagged() {
        let ds = vec![ line("∮ 我爱上了一个谎言♬ ∮"), line("♫ 啦啦啦 ♫") ];
        let issues = check_song_symbol(&ds);
        // 含 ∮ 但也含 ♬ 的行同样提示；♫ 行也提示
        assert_eq!(issues.len(), 2);
    }

    #[test]
    fn standard_integral_symbol_not_flagged() {
        // 规范的 ∮ 包裹行不提示
        let ds = vec![ line("∮ 有一天当你想唱歌时 ∮"), line("普通台词") ];
        assert!(check_song_symbol(&ds).is_empty());
    }

    #[test]
    fn plain_line_not_flagged() {
        let ds = vec![ line("你好世界"), line("#歌词#") ];
        assert!(check_song_symbol(&ds).is_empty());
    }
}
