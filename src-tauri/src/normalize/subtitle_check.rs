use crate::normalize::subtitle::{parse_dialogues, parse_time_cs, Dialogue, SEPARATOR};
pub use crate::normalize::subtitle::Issue;

/// 对标准化后文本做质检，返回可疑行。移植 SubtitlesSearch 的核心规则子集。
pub fn check(content: &str) -> Vec<Issue> {
    let dialogues = parse_dialogues(content);
    let mut issues = Vec::new();
    for (i, d) in dialogues.iter().enumerate() {
        // 规则2：可疑标点组合
        for bad in [".,", ",.", "--", "  ", ".!", ".?", "!.", "?."] {
            if d.text.contains(bad) {
                issues.push(Issue { line: i + 1, kind: format!("可疑标点[{bad}]"), text: d.text.clone() });
            }
        }
        // 规则3：双语结构缺失（含中文但无分隔符与英文）——仅提示
        let has_cjk = d.text.chars().any(|c| ('\u{4e00}'..='\u{9fa5}').contains(&c));
        if has_cjk && !d.text.contains(SEPARATOR) {
            issues.push(Issue { line: i + 1, kind: "缺英文行".into(), text: d.text.clone() });
        }
    }
    issues
}

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

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn flags_suspicious_punct() {
        let ass = "[Events]\nDialogue: 0,0:00:01.00,0:00:02.00,Default,,0,0,0,,坏  标点\\N{\\fnArial\\fs30}bad\n";
        let issues = check(ass);
        assert!(issues.iter().any(|i| i.kind.contains("可疑标点")));
    }
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
