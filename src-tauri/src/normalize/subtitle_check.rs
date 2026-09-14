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
