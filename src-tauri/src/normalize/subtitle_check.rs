use crate::normalize::subtitle::{parse_dialogues, SEPARATOR};
pub use crate::normalize::subtitle::Issue;

/// 对标准化后文本做质检，返回可疑行。移植 SubtitlesSearch 的核心规则子集。
pub fn check(content: &str) -> Vec<Issue> {
    let dialogues = parse_dialogues(content);
    let mut issues = Vec::new();
    for (i, d) in dialogues.iter().enumerate() {
        // 规则1：时间轴逆序（当前 start < 上一条 end 记为异常）
        if i > 0 {
            let prev_end = &dialogues[i - 1].end;
            if d.start.as_str() < prev_end.as_str() {
                issues.push(Issue { line: i + 1, kind: "时间轴逆序".into(), text: d.text.clone() });
            }
        }
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

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn flags_suspicious_punct() {
        let ass = "[Events]\nDialogue: 0,0:00:01.00,0:00:02.00,Default,,0,0,0,,坏  标点\\N{\\fnArial\\fs30}bad\n";
        let issues = check(ass);
        assert!(issues.iter().any(|i| i.kind.contains("可疑标点")));
    }
    #[test]
    fn flags_time_reversal() {
        let ass = "[Events]\n\
Dialogue: 0,0:00:05.00,0:00:09.00,Default,,0,0,0,,一\\N{\\fnArial\\fs30}one\n\
Dialogue: 0,0:00:03.00,0:00:04.00,Default,,0,0,0,,二\\N{\\fnArial\\fs30}two\n";
        let issues = check(ass);
        assert!(issues.iter().any(|i| i.kind == "时间轴逆序"));
    }
}
