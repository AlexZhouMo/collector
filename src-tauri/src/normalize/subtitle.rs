pub const SEPARATOR: &str = "\\N{\\fnArial\\fs30}";

/// 一条对白：时间戳区间 + 文本（可能含中英，用 SEPARATOR 分隔）。
#[derive(Debug, Clone)]
pub struct Dialogue {
    pub start: String,
    pub end: String,
    pub text: String,
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
