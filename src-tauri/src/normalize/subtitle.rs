use crate::normalize::special_chars::clean_special;
use crate::normalize::punct::{cn_punct, en_punct};
use crate::normalize::dialogue::{regularize_dash, regularize_markers};
use crate::normalize::classify::classify_style;
use crate::normalize::subtitle_check::{check_timeline_cross, check_monolingual, check_song_symbol};

pub const SEPARATOR: &str = "\\N{\\fnArial\\fs30}";

/// 质检/合并过程中发现的可疑行。
#[derive(Debug, Clone, serde::Serialize)]
pub struct Issue {
    pub line: usize,
    pub kind: String,
    pub text: String,
}

/// 一条对白：时间戳区间 + 文本（可能含中英，用 SEPARATOR 分隔）。
#[derive(Debug, Clone)]
pub struct Dialogue {
    pub start: String,
    pub end: String,
    pub text: String,
}

/// 时间戳 "H:MM:SS.CS" → 厘秒总数（centiseconds）。非法返回 None。
pub fn parse_time_cs(t: &str) -> Option<u32> {
    let (hms, cs) = t.split_once('.')?;
    let parts: Vec<&str> = hms.split(':').collect();
    if parts.len() != 3 { return None; }
    let h: u32 = parts[0].trim().parse().ok()?;
    let m: u32 = parts[1].parse().ok()?;
    let s: u32 = parts[2].parse().ok()?;
    let c: u32 = cs.parse().ok()?;
    Some(((h * 60 + m) * 60 + s) * 100 + c)
}

impl Dialogue {
    /// 中文段（SEPARATOR 之前，或无 SEPARATOR 时的全部）。
    pub fn zh(&self) -> String {
        match self.text.split_once(SEPARATOR) {
            Some((z, _)) => z.trim().to_string(),
            None => self.text.trim().to_string(),
        }
    }
    /// 英文段（SEPARATOR 之后）；无则 None。
    pub fn en(&self) -> Option<String> {
        self.text.split_once(SEPARATOR).map(|(_, e)| e.trim().to_string())
    }
    /// 由中文段 + 可选英文段重建 text。
    pub fn rebuild(zh: &str, en: Option<&str>) -> String {
        match en {
            Some(e) if !e.is_empty() => format!("{zh}{SEPARATOR}{e}"),
            _ => zh.to_string(),
        }
    }
}

/// 判断一段文本是否含中文。
fn has_cjk(s: &str) -> bool {
    s.chars().any(|c| ('\u{4e00}'..='\u{9fa5}').contains(&c))
}
/// 把同条内任意 `\N{...}` 旧分隔统一为标准 SEPARATOR（拆中英）。
fn unify_same_row_sep(text: &str) -> String {
    if let Some(p) = text.find("\\N{") {
        if let Some(q) = text[p..].find('}') {
            let head = &text[..p];
            let tail = &text[p + q + 1..];
            return format!("{}{}{}", head.trim(), SEPARATOR, tail.trim());
        }
    }
    text.to_string()
}

const NEAR_MISS_CS: u32 = 50; // 0.5s

/// 合并中英：先统一同条分隔；再把时间轴完全相同的多条合并（中文在前）。
/// 相邻两条时间轴差 ≤0.5s 且一中一英但未完全相同 → 提示，不合并。
pub fn merge_bilingual(dialogues: Vec<Dialogue>) -> (Vec<Dialogue>, Vec<Issue>) {
    let mut issues = Vec::new();
    let ds: Vec<Dialogue> = dialogues.into_iter().map(|mut d| {
        d.text = unify_same_row_sep(&d.text);
        d
    }).collect();
    let mut out: Vec<Dialogue> = Vec::new();
    let mut i = 0;
    while i < ds.len() {
        let mut group = vec![&ds[i]];
        let mut j = i + 1;
        while j < ds.len() && ds[j].start == ds[i].start && ds[j].end == ds[i].end {
            group.push(&ds[j]); j += 1;
        }
        if group.len() == 1 {
            out.push(ds[i].clone());
        } else {
            let zh_first = group.iter().find(|g| has_cjk(&g.text)).copied().unwrap_or(group[0]);
            let en_part = group.iter().find(|g| !has_cjk(&g.text)).map(|g| g.text.clone());
            let zh_seg = zh_first.text.split(SEPARATOR).next().unwrap_or(&zh_first.text).trim();
            out.push(Dialogue {
                start: ds[i].start.clone(), end: ds[i].end.clone(),
                text: Dialogue::rebuild(zh_seg, en_part.as_deref()),
            });
            if group.len() > 2 {
                issues.push(Issue { line: out.len(), kind: "同时间轴多于2条".into(), text: ds[i].text.clone() });
            }
        }
        i = j.max(i + 1);
    }
    for k in 1..out.len() {
        let (a, b) = (&out[k - 1], &out[k]);
        let complementary = has_cjk(&a.text) != has_cjk(&b.text);
        if complementary {
            if let (Some(as_), Some(ae), Some(bs), Some(be)) =
                (parse_time_cs(&a.start), parse_time_cs(&a.end), parse_time_cs(&b.start), parse_time_cs(&b.end)) {
                let ds_ = as_.abs_diff(bs); let de_ = ae.abs_diff(be);
                if (ds_ != 0 || de_ != 0) && ds_ <= NEAR_MISS_CS && de_ <= NEAR_MISS_CS {
                    issues.push(Issue { line: k + 1, kind: "疑似未合并中英".into(), text: b.text.clone() });
                }
            }
        }
    }
    (out, issues)
}

/// 去除 UTF-8 BOM，统一换行为 \n。
pub fn preprocess(raw: &str) -> String {
    raw.trim_start_matches('\u{feff}')
        .replace("\r\n", "\n")
        .replace('\r', "\n")
}

/// 判断一条 Dialogue 文本是否无实义内容：去掉 {\...} 特效标签、\N 换行、\h 硬空格、
/// 空白后为空 → 视为空文本行（特效/占位行），应跳过。
fn is_blank_dialogue(text: &str) -> bool {
    // 遍历字符，遇 '{' 进入跳过模式直到 '}'，其余字符收集。
    let mut s = String::with_capacity(text.len());
    let mut skipping = false;
    for c in text.chars() {
        if skipping {
            if c == '}' {
                skipping = false;
            }
            continue;
        }
        if c == '{' {
            skipping = true;
            continue;
        }
        s.push(c);
    }
    // 去掉 \N \h 和所有空白
    let s = s
        .replace("\\N", "")
        .replace("\\h", "")
        .replace(char::is_whitespace, "");
    s.is_empty()
}

/// 从 .ass 文本解析出所有 Dialogue 行（保留时间与文本主体）。
pub fn parse_dialogues(content: &str) -> Vec<Dialogue> {
    let mut out = Vec::new();
    for line in preprocess(content).lines() {
        if !line.starts_with("Dialogue:") {
            continue;
        }
        let rest = &line["Dialogue:".len()..];
        let parts: Vec<&str> = rest.splitn(10, ',').collect();
        if parts.len() < 10 {
            continue;
        }
        let text = parts[9].to_string();
        if is_blank_dialogue(&text) {
            continue;
        }
        out.push(Dialogue {
            start: parts[1].trim().to_string(),
            end: parts[2].trim().to_string(),
            text,
        });
    }
    out
}

/// 生成统一 ASS 头（样式取自 demo：Default/Title/Note，MarginV 控制字幕在下方）。
pub fn build_header() -> String {
    let mut s = String::new();
    s.push_str("[Script Info]\nPlayResX: 1280\nPlayResY: 720\n\n");
    s.push_str("[V4+ Styles]\n");
    s.push_str("Format: Name, Fontname, Fontsize, PrimaryColour, SecondaryColour, OutlineColour, BackColour, Bold, Italic, Underline, StrikeOut, ScaleX, ScaleY, Spacing, Angle, BorderStyle, Outline, Shadow, Alignment, MarginL, MarginR, MarginV, Encoding\n");
    s.push_str("Style: Default,SimHei,32,&H00FFFFFF,&H0000FFFF,&H00000000,&H80000000,0,0,0,0,100,100,0,0,1,2,2,2,0,0,10,1\n");
    s.push_str("Style: Title,SimHei,35,&H00FFFFFF,&H0000FFFF,&H00000000,&H80000000,0,0,0,0,100,100,0,0,1,2,2,2,0,0,30,1\n");
    s.push_str("Style: Note,SimHei,32,&H00FFFFFF,&H0000FFFF,&H00000000,&H80000000,0,0,0,0,100,100,0,0,1,2,2,2,0,0,40,1\n\n");
    s.push_str("[Events]\n");
    s.push_str("Format: Layer, Start, End, Style, Name, MarginL, MarginR, MarginV, Effect, Text\n");
    s
}

/// 完整校准 pipeline：返回 (标准化 .ass 文本, 提示列表)。
pub fn format_ass(content: &str, _char_map: &[(String, String)]) -> (String, Vec<Issue>) {
    let mut issues = Vec::new();
    let parsed = parse_dialogues(content);
    let (merged, mut merge_issues) = merge_bilingual(parsed);
    issues.append(&mut merge_issues);
    issues.extend(check_timeline_cross(&merged));
    issues.extend(check_monolingual(&merged));
    issues.extend(check_song_symbol(&merged));

    let mut out = build_header();
    for (i, d) in merged.iter().enumerate() {
        let zh_raw = d.zh();
        let en_raw = d.en();
        // 中文段：clean_special → cn_punct → regularize_markers → regularize_dash
        let mut zh = clean_special(&zh_raw);
        zh = cn_punct(&zh);
        zh = regularize_markers(&zh);
        zh = regularize_dash(&zh);
        // 英文段：clean_special → en_punct → regularize_dash（不做括号全角）
        let en = en_raw.map(|e| {
            let mut t = clean_special(&e);
            t = en_punct(&t);
            regularize_dash(&t)
        });
        let style = classify_style(&zh, en.is_some());
        let text = Dialogue::rebuild(&zh, en.as_deref());
        out.push_str(&format!(
            "Dialogue: 0,{},{},{},0,0,0,0,,{}\n",
            d.start, d.end, style.name(), text
        ));
        // 残留可疑标点提示。`.,` 常见于英文缩写（如 D.C., S.O.S., Jan.,），
        // 属正常情况——仅当点号前不是字母时才视为可疑，避免误报缩写。
        for bad in [",.", "  "] {
            if text.contains(bad) {
                issues.push(Issue { line: i + 1, kind: format!("残留可疑标点[{bad}]"), text: text.clone() });
            }
        }
        if has_abnormal_dot_comma(&text) {
            issues.push(Issue { line: i + 1, kind: "残留可疑标点[.,]".into(), text: text.clone() });
        }
    }
    (out, issues)
}

/// `.,` 是否为「异常」（需提示）：英文缩写点后的逗号（如 `D.C.,`、`Jan.,`）属正常，
/// 其点号前必是字母；仅当某处 `.,` 的点号前不是 ASCII 字母（或在行首）时才视为异常。
fn has_abnormal_dot_comma(text: &str) -> bool {
    let chars: Vec<char> = text.chars().collect();
    for i in 0..chars.len() {
        if chars[i] == '.' && i + 1 < chars.len() && chars[i + 1] == ',' {
            let prev_is_letter = i > 0 && chars[i - 1].is_ascii_alphabetic();
            if !prev_is_letter {
                return true;
            }
        }
    }
    false
}

#[cfg(test)]
mod build_tests {
    use super::*;
    #[test]
    fn format_produces_header_and_dialogue() {
        let ass = "[Events]\nDialogue: 0,0:00:01.00,0:00:02.00,Default,,0,0,0,,你好!\\N{\\fnArial\\fs30}Hi!\n";
        let (out, _) = format_ass(ass, &[]);
        assert!(out.contains("[V4+ Styles]"));
        assert!(out.contains("Style: Default,SimHei,32"));
        assert!(out.contains("你好！"));
        assert!(out.contains("Dialogue: 0,0:00:01.00,0:00:02.00,"));
    }
    #[test]
    fn fullwidth_applies_to_chinese() {
        let ass = "[Events]\nDialogue: 0,0:00:01.00,0:00:02.00,Default,,0,0,0,,你好!\n";
        let (out, _) = format_ass(ass, &[]);
        assert!(out.contains("你好！"));
    }
    #[test]
    fn english_side_stays_halfwidth() {
        // 中英同条：中文段应全角化，英文段必须保持半角（逗号不能变，、不能被全角括号污染）
        let ass = "[Events]\nDialogue: 0,0:00:01.00,0:00:02.00,Default,,0,0,0,,你好,世界(测试)\\N{\\fnArial\\fs30}Hello,world(test)\n";
        let (out, _) = format_ass(ass, &[]);
        // 中文段：全角逗号 + 全角括号
        assert!(out.contains("你好，世界"), "中文段逗号应全角: {out}");
        assert!(out.contains("（测试）"), "中文段括号应全角: {out}");
        // 英文段：半角逗号(带空格)、半角括号，绝不能出现全角化
        assert!(out.contains("Hello, world"), "英文段逗号应半角: {out}");
        assert!(out.contains("(test)"), "英文段括号应保持半角: {out}");
        assert!(!out.contains("Hello，"), "英文段逗号被误全角化: {out}");
        assert!(!out.contains("（test"), "英文段括号被误全角化: {out}");
    }

    #[test]
    fn monolingual_bare_line_flagged_in_pipeline() {
        // 双语文件（≥5 条带译文对白、时间轴各异避免被合并）里出现裸中文行「中国 北京」
        // → format_ass 应产出「疑似漏译(仅中文)」提示
        let mut ass = String::from("[Events]\n");
        for i in 0..5 {
            ass.push_str(&format!(
                "Dialogue: 0,0:00:{i:02}.00,0:00:{i:02}.50,Default,,0,0,0,,你好\\N{{\\fnArial\\fs30}}Hi\n"));
        }
        ass.push_str("Dialogue: 0,0:00:30.00,0:00:31.00,Default,,0,0,0,,中国 北京\n");
        let (_, issues) = format_ass(&ass, &[]);
        assert!(issues.iter().any(|i| i.kind == "疑似漏译(仅中文)"),
            "应检出裸中文行: {issues:?}");
    }

    #[test]
    fn monolingual_wrapped_note_not_flagged_in_pipeline() {
        // 双语文件里被括号包裹的旁白（中国 北京）→ 正常，不产漏译提示
        let mut ass = String::from("[Events]\n");
        for i in 0..5 {
            ass.push_str(&format!(
                "Dialogue: 0,0:00:{i:02}.00,0:00:{i:02}.50,Default,,0,0,0,,你好\\N{{\\fnArial\\fs30}}Hi\n"));
        }
        ass.push_str("Dialogue: 0,0:00:30.00,0:00:31.00,Default,,0,0,0,,（中国 北京）\n");
        let (_, issues) = format_ass(&ass, &[]);
        assert!(!issues.iter().any(|i| i.kind.contains("漏译")),
            "被括号包裹不应报漏译: {issues:?}");
    }
}

#[cfg(test)]
mod time_tests {
    use super::*;
    #[test]
    fn parse_time_to_cs() {
        assert_eq!(parse_time_cs("0:00:42.66"), Some(4266));
        assert_eq!(parse_time_cs("1:02:03.00"), Some(372300));
        assert_eq!(parse_time_cs("bad"), None);
    }
    #[test]
    fn dialogue_zh_en_split() {
        let d = Dialogue { start:"0:00:01.00".into(), end:"0:00:02.00".into(),
            text: format!("中文{}English", SEPARATOR) };
        assert_eq!(d.zh(), "中文");
        assert_eq!(d.en(), Some("English".to_string()));
        let d2 = Dialogue { start:"".into(), end:"".into(), text:"纯中文".into() };
        assert_eq!(d2.en(), None);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn strips_bom_and_crlf() {
        let s = preprocess("\u{feff}a\r\nb\r\n");
        assert_eq!(s, "a\nb\n");
    }
    #[test]
    fn parses_dialogue_lines() {
        let ass = "\u{feff}[Events]\r\nDialogue: 0,0:00:02.20,0:00:05.68,Default,,0,0,0,,中文\\N{\\fnArial\\fs30}English\r\n";
        let ds = parse_dialogues(ass);
        assert_eq!(ds.len(), 1);
        assert_eq!(ds[0].start, "0:00:02.20");
        assert!(ds[0].text.contains(SEPARATOR));
    }
}

#[cfg(test)]
mod blank_tests {
    use super::*;
    #[test]
    fn blank_dialogue_detection() {
        assert!(is_blank_dialogue(""));
        assert!(is_blank_dialogue("{\\pos(100,200)}"));
        assert!(is_blank_dialogue("{\\shad1}\\N{\\fnArial}"));
        assert!(is_blank_dialogue("   \\N  "));
        assert!(!is_blank_dialogue("你好"));
        assert!(!is_blank_dialogue("{\\b0}Hello"));
        assert!(!is_blank_dialogue("中文\\N{\\fnArial}English"));
    }
    #[test]
    fn parse_skips_blank_dialogues() {
        let ass = "[Events]\n\
Dialogue: 0,0:00:01.00,0:00:02.00,Default,,0,0,0,,你好\n\
Dialogue: 0,0:00:01.50,0:00:02.50,Default,,0,0,0,,{\\pos(1,2)}\n\
Dialogue: 0,0:00:03.00,0:00:04.00,Default,,0,0,0,,世界\n";
        let ds = parse_dialogues(ass);
        assert_eq!(ds.len(), 2); // 空文本行被跳过
        assert_eq!(ds[0].text, "你好");
        assert_eq!(ds[1].text, "世界");
    }
}

#[cfg(test)]
mod merge_tests {
    use super::*;
    fn d(s:&str,e:&str,t:&str)->Dialogue{Dialogue{start:s.into(),end:e.into(),text:t.into()}}
    #[test]
    fn merge_same_timeline_zh_en() {
        let input = vec![
            d("0:00:01.00","0:00:02.00","你好"),
            d("0:00:01.00","0:00:02.00","Hello"),
        ];
        let (out, issues) = merge_bilingual(input);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].zh(), "你好");
        assert_eq!(out[0].en(), Some("Hello".to_string()));
        assert!(issues.is_empty());
    }
    #[test]
    fn split_same_row_reunifies() {
        let input = vec![ d("0:00:01.00","0:00:02.00","中文\\N{\\fnX}英文") ];
        let (out, _) = merge_bilingual(input);
        assert_eq!(out[0].zh(), "中文");
        assert_eq!(out[0].en(), Some("英文".to_string()));
    }
    #[test]
    fn near_miss_reports_not_merges() {
        let input = vec![
            d("0:00:01.00","0:00:02.00","你好"),
            d("0:00:01.30","0:00:02.20","Hello"),
        ];
        let (out, issues) = merge_bilingual(input);
        assert_eq!(out.len(), 2);
        assert!(issues.iter().any(|i| i.kind == "疑似未合并中英"));
    }
}

#[cfg(test)]
mod dot_comma_tests {
    use super::*;
    #[test]
    fn abbreviation_dot_comma_not_flagged() {
        // 缩写点+逗号：点号前是字母 → 正常，不报
        assert!(!has_abnormal_dot_comma("Washington D.C., is nice"));
        assert!(!has_abnormal_dot_comma("S.O.S., help"));
        assert!(!has_abnormal_dot_comma("in Jan., you win"));
    }
    #[test]
    fn abnormal_dot_comma_flagged() {
        // 点号前非字母（行首 / 空格 / 数字后省略号误接逗号等）→ 异常，报
        assert!(has_abnormal_dot_comma("word .,broken"));  // 点前是空格
        assert!(has_abnormal_dot_comma(".,leading"));      // 行首
    }
}
