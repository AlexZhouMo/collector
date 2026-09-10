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
        out.push(Dialogue {
            start: parts[1].trim().to_string(),
            end: parts[2].trim().to_string(),
            text: parts[9].to_string(),
        });
    }
    out
}

/// 中文标点全角化：把中文部分的半角标点转为全角。作用于 SEPARATOR 之前的中文段。
pub fn fullwidth_chinese_punct(chinese: &str) -> String {
    chinese
        .replace(", ", "，")
        .replace(',', "，")
        .replace("! ", "！")
        .replace('!', "！")
        .replace("? ", "？")
        .replace('?', "？")
        .replace(": ", "：")
        .replace(':', "：")
        .replace("...", "…")
        .replace(". ", "。")
}

/// 把一条对白的文本按 SEPARATOR 拆成 (中文, 英文可选)，对中文做全角化后重组。
pub fn normalize_text(text: &str) -> String {
    match text.split_once(SEPARATOR) {
        Some((zh, en)) => format!(
            "{}{}{}",
            fullwidth_chinese_punct(zh.trim()),
            SEPARATOR,
            en.trim()
        ),
        None => fullwidth_chinese_punct(text.trim()),
    }
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

/// 应用可配置字符映射表（默认空）后，产出标准化后的完整 .ass 文本。
pub fn format_ass(content: &str, char_map: &[(String, String)]) -> String {
    let dialogues = parse_dialogues(content);
    let mut out = build_header();
    for d in dialogues {
        let mut text = normalize_text(&d.text);
        for (from, to) in char_map {
            text = text.replace(from, to);
        }
        out.push_str(&format!(
            "Dialogue: 0,{},{},Default,0,0,0,0,,{}\n",
            d.start, d.end, text
        ));
    }
    out
}

#[cfg(test)]
mod norm_tests {
    use super::*;
    #[test]
    fn fullwidth_punct_on_chinese_side_only() {
        let t = "你好, 世界!\\N{\\fnArial\\fs30}Hello, world!";
        let r = normalize_text(t);
        assert!(r.starts_with("你好，世界！"));
        assert!(r.ends_with("Hello, world!")); // 英文侧不动
    }
}

#[cfg(test)]
mod build_tests {
    use super::*;
    #[test]
    fn format_produces_header_and_dialogue() {
        let ass = "[Events]\nDialogue: 0,0:00:01.00,0:00:02.00,Default,,0,0,0,,你好!\\N{\\fnArial\\fs30}Hi!\n";
        let out = format_ass(ass, &[]);
        assert!(out.contains("[V4+ Styles]"));
        assert!(out.contains("Style: Default,SimHei,32"));
        assert!(out.contains("你好！"));
        assert!(out.contains("Dialogue: 0,0:00:01.00,0:00:02.00,Default"));
    }
    #[test]
    fn char_map_applies() {
        let ass = "[Events]\nDialogue: 0,0:00:01.00,0:00:02.00,Default,,0,0,0,,錯字\n";
        let out = format_ass(ass, &[("錯".to_string(), "错".to_string())]);
        assert!(out.contains("错字"));
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
