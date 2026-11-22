use crate::normalize::special_chars::{clean_special, has_garbled, restore_big5_punct, strip_invisible};
use crate::normalize::punct::{cn_punct, en_punct};
use crate::normalize::dialogue::{regularize_dash, regularize_markers};
use crate::normalize::classify::{classify, normalize_note, normalize_title, strip_space_before_title_mark, Style};
use crate::normalize::subtitle_check::{check_timeline_cross, check_monolingual, check_song_symbol, has_latin, LEGACY_SONG_MARK, SONG_MARK};

pub const SEPARATOR: &str = "\\N{\\fnArial\\fs30}";

/// 质检/合并过程中发现的可疑行。
#[derive(Debug, Clone, serde::Serialize)]
pub struct Issue {
    pub line: usize,
    pub kind: String,
    pub text: String,
    /// 该条字幕的起止时间（ASS 原样字符串）。
    pub start: String,
    pub end: String,
    /// 对应原文物理行号（1-based，可能多行）。用于编辑器定位。
    pub src_lines: Vec<usize>,
}

/// 一条对白：时间戳区间 + 文本（可能含中英，用 SEPARATOR 分隔）。
#[derive(Debug, Clone)]
pub struct Dialogue {
    pub start: String,
    pub end: String,
    pub text: String,
    /// 这条对白来自原文的物理行号（1-based）。合并后可能含多个。
    pub src_lines: Vec<usize>,
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
pub(crate) fn has_cjk(s: &str) -> bool {
    s.chars().any(|c| ('\u{4e00}'..='\u{9fa5}').contains(&c))
}
/// ASS 样式复位标签。字幕组常挂在行尾/行中，校准后统一样式，已无意义，输出前全部删除。
const RESET_TAG: &str = "{\\r}";

/// 去掉 `{...}` 特效/覆盖标签，只留可见文字（用于语言判断，避免字体名里的汉字/字母干扰）。
pub(crate) fn strip_override_tags(text: &str) -> String {
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
    s
}

/// 可见文字是否含中文（忽略 `{...}` 标签内容）。
fn visible_has_cjk(text: &str) -> bool {
    has_cjk(&strip_override_tags(text))
}

/// 可见文字是否含拉丁字母（忽略 `{...}` 标签，以及 `\N` `\h` 转义自身的字母）。
fn visible_has_latin(text: &str) -> bool {
    has_latin(&strip_override_tags(text).replace("\\N", "").replace("\\h", ""))
}

/// 把同条内的中英分隔统一为标准 SEPARATOR（拆中英）：
/// 1) 任意 `\N{...}` 旧分隔 → 标准分隔；
/// 2) 裸 `\N`（后面没有 `{...}`）：仅当其前含中文、其后不含中文且含拉丁字母，
///    即「中文\N英文」时才视为中英分隔；「中文\N中文」这类普通换行保持原样。
fn unify_same_row_sep(text: &str) -> String {
    if let Some(p) = text.find("\\N{") {
        if let Some(q) = text[p..].find('}') {
            let head = &text[..p];
            let tail = &text[p + q + 1..];
            return format!("{}{}{}", head.trim(), SEPARATOR, tail.trim());
        }
    }
    let mut from = 0;
    while let Some(rel) = text[from..].find("\\N") {
        let p = from + rel;
        let (head, tail) = (&text[..p], &text[p + 2..]);
        if visible_has_cjk(head) && !visible_has_cjk(tail) && visible_has_latin(tail) {
            return format!("{}{}{}", head.trim(), SEPARATOR, tail.trim());
        }
        from = p + 2;
    }
    text.to_string()
}

/// 把「时间轴完全相同但不相邻」的一中一英两条挪到相邻位置（落在先出现的那条处），
/// 交给后面的相邻合并。针对中文块、英文块分开存放的字幕。
/// 只处理干净情形：整个文件里该时间轴恰好两条、一条含中文一条只有英文、且都还没有译文分隔。
/// 同语言重复、三条及以上同轴的保持原样，避免误并。
fn pair_up_separated(ds: Vec<Dialogue>) -> Vec<Dialogue> {
    use std::collections::HashMap;
    let mut partner: HashMap<usize, usize> = HashMap::new(); // 先出现的 → 后出现的
    let mut moved = vec![false; ds.len()];
    {
        let mut by_time: HashMap<(&str, &str), Vec<usize>> = HashMap::new();
        for (i, d) in ds.iter().enumerate() {
            by_time.entry((d.start.as_str(), d.end.as_str())).or_default().push(i);
        }
        for idx in by_time.values() {
            if let [a, b] = idx[..] {
                if b - a > 1 && is_separated_zh_en(&ds[a], &ds[b]) {
                    partner.insert(a, b);
                    moved[b] = true;
                }
            }
        }
    }
    if partner.is_empty() {
        return ds;
    }
    let mut out = Vec::with_capacity(ds.len());
    for i in 0..ds.len() {
        if moved[i] {
            continue;
        }
        out.push(ds[i].clone());
        if let Some(&j) = partner.get(&i) {
            out.push(ds[j].clone());
        }
    }
    out
}

/// 可见文字是否含拉丁字母或数字：英文译文的判据，`1955.` 这类纯数字译文也算。
fn visible_has_latin_or_digit(text: &str) -> bool {
    visible_has_latin(text)
        || strip_override_tags(text).chars().any(|c| c.is_ascii_digit())
}

/// 两条是否构成「分开存放的一中一英」：都没有译文分隔，一条含中文，
/// 另一条不含中文且含拉丁字母或数字（年份等纯数字译文如 `1955.` 也要能配上）。
fn is_separated_zh_en(a: &Dialogue, b: &Dialogue) -> bool {
    if a.text.contains(SEPARATOR) || b.text.contains(SEPARATOR) {
        return false;
    }
    match (visible_has_cjk(&a.text), visible_has_cjk(&b.text)) {
        (true, false) => visible_has_latin_or_digit(&b.text),
        (false, true) => visible_has_latin_or_digit(&a.text),
        _ => false,
    }
}

const NEAR_MISS_CS: u32 = 50; // 0.5s

/// 合并中英：先统一同条分隔；再把时间轴完全相同的多条合并（中文在前），
/// 相邻的直接合并，不相邻的一中一英先配对挪到一起再合并。
/// 相邻两条时间轴差 ≤0.5s 且一中一英但未完全相同 → 提示，不合并。
pub fn merge_bilingual(dialogues: Vec<Dialogue>) -> (Vec<Dialogue>, Vec<Issue>) {
    let mut issues = Vec::new();
    let ds: Vec<Dialogue> = dialogues.into_iter().map(|mut d| {
        d.text = unify_same_row_sep(&d.text);
        d
    }).collect();
    let ds = pair_up_separated(ds);
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
            let zh_first = group.iter().find(|g| visible_has_cjk(&g.text)).copied().unwrap_or(group[0]);
            let en_part = group.iter().find(|g| !visible_has_cjk(&g.text)).map(|g| g.text.clone());
            let zh_seg = zh_first.text.split(SEPARATOR).next().unwrap_or(&zh_first.text).trim();
            let mut src_lines: Vec<usize> = group.iter().flat_map(|g| g.src_lines.clone()).collect();
            src_lines.sort_unstable();
            src_lines.dedup();
            out.push(Dialogue {
                start: ds[i].start.clone(), end: ds[i].end.clone(),
                text: Dialogue::rebuild(zh_seg, en_part.as_deref()),
                src_lines,
            });
            if group.len() > 2 {
                issues.push(Issue { line: out.len(), kind: "同时间轴多于2条".into(), text: ds[i].text.clone(), start: out.last().unwrap().start.clone(), end: out.last().unwrap().end.clone(), src_lines: out.last().unwrap().src_lines.clone() });
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
                    issues.push(Issue { line: k + 1, kind: "疑似未合并中英".into(), text: b.text.clone(), start: b.start.clone(), end: b.end.clone(), src_lines: b.src_lines.clone() });
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
    // 去掉 {...} 标签、\N \h 和所有空白
    let s = strip_override_tags(text)
        .replace("\\N", "")
        .replace("\\h", "")
        .replace(char::is_whitespace, "");
    s.is_empty()
}

/// 从 .ass 文本解析出所有 Dialogue 行（保留时间与文本主体）。
pub fn parse_dialogues(content: &str) -> Vec<Dialogue> {
    let mut out = Vec::new();
    for (idx, line) in preprocess(content).lines().enumerate() {
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
            src_lines: vec![idx + 1], // 物理行号 1-based
        });
    }
    out
}

/// 生成统一 ASS 头：四种样式 Bilingual（中英双行）/ Unilingual（普通单行）/ Title（书名）/ Note（注释）。
/// MarginV 控制字幕离底边的距离；Unilingual 与 Note 参数相同。
pub fn build_header() -> String {
    let mut s = String::new();
    s.push_str("[Script Info]\nPlayResX: 1280\nPlayResY: 720\n\n");
    s.push_str("[V4+ Styles]\n");
    s.push_str("Format: Name, Fontname, Fontsize, PrimaryColour, SecondaryColour, OutlineColour, BackColour, Bold, Italic, Underline, StrikeOut, ScaleX, ScaleY, Spacing, Angle, BorderStyle, Outline, Shadow, Alignment, MarginL, MarginR, MarginV, Encoding\n");
    s.push_str("Style: Bilingual,SimHei,32,&H00FFFFFF,&H0000FFFF,&H00000000,&H80000000,0,0,0,0,100,100,0,0,1,2,2,2,0,0,10,1\n");
    s.push_str("Style: Unilingual,SimHei,32,&H00FFFFFF,&H0000FFFF,&H00000000,&H80000000,0,0,0,0,100,100,0,0,1,2,2,2,0,0,40,1\n");
    s.push_str("Style: Title,SimHei,35,&H00FFFFFF,&H0000FFFF,&H00000000,&H80000000,0,0,0,0,100,100,0,0,1,2,2,2,0,0,30,1\n");
    s.push_str("Style: Note,SimHei,32,&H00FFFFFF,&H0000FFFF,&H00000000,&H80000000,0,0,0,0,100,100,0,0,1,2,2,2,0,0,40,1\n\n");
    s.push_str("[Events]\n");
    s.push_str("Format: Layer, Start, End, Style, Name, MarginL, MarginR, MarginV, Effect, Text\n");
    s
}

/// 完整校准 pipeline：返回 (标准化 .ass 文本, 提示列表)。
pub fn format_ass(content: &str, _char_map: &[(String, String)]) -> (String, Vec<Issue>) {
    let mut issues = Vec::new();
    // 先删掉所有 {\r}：既不留在输出里，也不干扰后面的中英拆分与合并。
    // 同时把旧的音乐符 ∮ 一对一换成 ♪（中英两段都换）。
    // 再往前一步先清字符层面的垃圾：删不可见的控制/格式字符（播放器会画成方框），
    // 把 Big5 标点转码遗留的私用区字符还原成标点。清完只剩空白的行按空行丢弃。
    let parsed: Vec<Dialogue> = parse_dialogues(content).into_iter().map(|mut d| {
        d.text = restore_big5_punct(&strip_invisible(&d.text))
            .replace(RESET_TAG, "").replace(LEGACY_SONG_MARK, SONG_MARK.encode_utf8(&mut [0; 4]));
        d
    }).filter(|d| !is_blank_dialogue(&d.text)).collect();
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
        zh = strip_space_before_title_mark(&zh);
        // 英文段为空等同于没有译文（rebuild 也按单行输出）
        let has_en = en.as_deref().is_some_and(|e| !e.is_empty());
        let classified = classify(&zh, has_en);
        zh = match classified.style {
            Style::Title => normalize_title(&zh),
            Style::Note => normalize_note(&zh),
            Style::Bilingual | Style::Unilingual => zh,
        };
        let text = Dialogue::rebuild(&zh, en.as_deref());
        out.push_str(&format!(
            "Dialogue: 0,{},{},{},0,0,0,0,,{}\n",
            d.start, d.end, classified.style.name(), text
        ));
        if let Some(doubt) = classified.doubt {
            issues.push(Issue { line: i + 1, kind: doubt.label().into(), text: text.clone(), start: d.start.clone(), end: d.end.clone(), src_lines: d.src_lines.clone() });
        }
        if has_suspicious_punct(&text) {
            issues.push(Issue { line: i + 1, kind: PUNCT_ISSUE_KIND.into(), text: text.clone(), start: d.start.clone(), end: d.end.clone(), src_lines: d.src_lines.clone() });
        }
        if has_abnormal_font(&text) {
            issues.push(Issue { line: i + 1, kind: "异常字体".into(), text: text.clone(), start: d.start.clone(), end: d.end.clone(), src_lines: d.src_lines.clone() });
        }
        if has_garbled(&text) {
            issues.push(Issue { line: i + 1, kind: "疑似乱码".into(), text: text.clone(), start: d.start.clone(), end: d.end.clone(), src_lines: d.src_lines.clone() });
        }
    }
    (out, issues)
}

/// 是否含「异常字体」：除标准中英分隔符自带的 `{\fnArial\fs30}` 外，正文里还残留 `{\fn…` 字体覆盖标签。
fn has_abnormal_font(text: &str) -> bool {
    text.replace(SEPARATOR, "").contains("{\\fn")
}

/// 标点类提示的统一类型名。所有与标点符号有关的检查都归到这一类，一行只报一条；
/// 不再按具体标点细分成「残留可疑标点[,.]」之类的子类型。
/// 以后新增标点相关规则时，优先往 `has_suspicious_punct` 里加，而不是另起一种提示类型。
const PUNCT_ISSUE_KIND: &str = "残留可疑标点";

/// 校准后的文本里是否还有可疑标点。目前包含三条规则：
/// `,.`；连续两个空格；异常的 `.,`（见 `has_abnormal_dot_comma`）。
fn has_suspicious_punct(text: &str) -> bool {
    text.contains(",.") || text.contains("  ") || has_abnormal_dot_comma(text)
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
        assert!(out.contains("Style: Bilingual,SimHei,32"));
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
        // → format_ass 应产出「疑似漏译」提示
        let mut ass = String::from("[Events]\n");
        for i in 0..5 {
            ass.push_str(&format!(
                "Dialogue: 0,0:00:{i:02}.00,0:00:{i:02}.50,Default,,0,0,0,,你好\\N{{\\fnArial\\fs30}}Hi\n"));
        }
        ass.push_str("Dialogue: 0,0:00:30.00,0:00:31.00,Default,,0,0,0,,中国 北京\n");
        let (_, issues) = format_ass(&ass, &[]);
        assert!(issues.iter().any(|i| i.kind == "疑似漏译"),
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

    #[test]
    fn pipeline_strips_invisible_and_restores_big5_punct() {
        let ass = "[Events]\n\
Dialogue: 0,0:00:01.00,0:00:02.00,Default,,0,0,0,,- 你好 - 抱歉\\N{\\fnArial\\fs30}- Hi. - \u{202D}Sorry.\u{7F}\n\
Dialogue: 0,0:00:03.00,0:00:04.00,Default,,0,0,0,,问题是\u{E4D1}\n\
Dialogue: 0,0:00:05.00,0:00:06.00,Default,,0,0,0,,\u{200B}\u{202D}\n";
        let (out, issues) = format_ass(ass, &[]);
        assert!(out.contains("- Hi. - Sorry.\n"), "{out}");
        assert!(out.contains(",问题是…\n"), "{out}");
        // 清完只剩空白的行整条丢弃
        assert_eq!(out.matches("Dialogue:").count(), 2, "{out}");
        assert!(!out.chars().any(|c| matches!(c, '\u{202D}' | '\u{7F}' | '\u{200B}' | '\u{E4D1}')), "{out}");
        assert!(!issues.iter().any(|i| i.kind == "疑似乱码"), "{issues:?}");
    }

    #[test]
    fn pipeline_flags_leftover_garbled_text() {
        let ass = "[Events]\n\
Dialogue: 0,0:00:01.00,0:00:02.00,Default,,0,0,0,,瞬间接\u{E769}剂\n\
Dialogue: 0,0:00:03.00,0:00:04.00,Default,,0,0,0,,快堵住入□\n\
Dialogue: 0,0:00:05.00,0:00:06.00,Default,,0,0,0,,一切正常\n";
        let (_, issues) = format_ass(ass, &[]);
        let garbled: Vec<&Issue> = issues.iter().filter(|i| i.kind == "疑似乱码").collect();
        assert_eq!(garbled.len(), 2, "{issues:?}");
        assert_eq!(garbled[0].src_lines, vec![2]);
        assert_eq!(garbled[1].src_lines, vec![3]);
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
            text: format!("中文{}English", SEPARATOR), src_lines: vec![1] };
        assert_eq!(d.zh(), "中文");
        assert_eq!(d.en(), Some("English".to_string()));
        let d2 = Dialogue { start:"".into(), end:"".into(), text:"纯中文".into(), src_lines: vec![1] };
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
mod src_line_tests {
    use super::*;
    #[test]
    fn parse_records_physical_line_numbers() {
        // 第1行 [Events]、第2行 Dialogue、第3行空文本特效行(跳过)、第4行 Dialogue
        let ass = "[Events]\n\
Dialogue: 0,0:00:01.00,0:00:02.00,Default,,0,0,0,,你好\n\
Dialogue: 0,0:00:01.50,0:00:02.50,Default,,0,0,0,,{\\pos(1,2)}\n\
Dialogue: 0,0:00:03.00,0:00:04.00,Default,,0,0,0,,世界\n";
        let ds = parse_dialogues(ass);
        assert_eq!(ds.len(), 2);
        assert_eq!(ds[0].src_lines, vec![2]); // 「你好」在物理第2行
        assert_eq!(ds[1].src_lines, vec![4]); // 「世界」在物理第4行(第3行被跳过但仍占行号)
    }
}

#[cfg(test)]
mod merge_tests {
    use super::*;
    fn d(s:&str,e:&str,t:&str)->Dialogue{Dialogue{start:s.into(),end:e.into(),text:t.into(),src_lines:vec![1]}}
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
mod separated_tests {
    use super::*;
    fn d(s: &str, e: &str, t: &str, line: usize) -> Dialogue {
        Dialogue { start: s.into(), end: e.into(), text: t.into(), src_lines: vec![line] }
    }

    #[test]
    fn non_adjacent_same_timeline_pair_merged() {
        // 英文块在前、中文块在后（各自成段），时间轴逐条相同 → 两两合并，落在先出现的位置
        let input = vec![
            d("0:00:01.00", "0:00:02.00", "Hello", 1),
            d("0:00:03.00", "0:00:04.00", "Bye", 2),
            d("0:00:01.00", "0:00:02.00", "你好", 3),
            d("0:00:03.00", "0:00:04.00", "再见", 4),
        ];
        let (out, issues) = merge_bilingual(input);
        assert_eq!(out.len(), 2);
        assert_eq!(out[0].text, format!("你好{SEPARATOR}Hello"));
        assert_eq!(out[1].text, format!("再见{SEPARATOR}Bye"));
        assert_eq!(out[0].src_lines, vec![1, 3]);
        assert!(issues.is_empty(), "{issues:?}");
    }

    #[test]
    fn non_adjacent_same_language_not_merged() {
        // 同轴但都是中文（或都是英文）→ 不是中英对，原样保留，不丢行
        let input = vec![
            d("0:00:01.00", "0:00:02.00", "你好", 1),
            d("0:00:03.00", "0:00:04.00", "中间", 2),
            d("0:00:01.00", "0:00:02.00", "你好啊", 3),
        ];
        let (out, _) = merge_bilingual(input);
        assert_eq!(out.len(), 3);
        assert_eq!(out[2].text, "你好啊");
    }

    #[test]
    fn non_adjacent_already_bilingual_not_merged() {
        // 其中一条已是双语 → 不再并入别的行
        let input = vec![
            d("0:00:01.00", "0:00:02.00", "你好\\N{\\fnArial\\fs30}Hello", 1),
            d("0:00:03.00", "0:00:04.00", "中间", 2),
            d("0:00:01.00", "0:00:02.00", "Hi there", 3),
        ];
        let (out, _) = merge_bilingual(input);
        assert_eq!(out.len(), 3);
    }

    #[test]
    fn three_rows_on_same_timeline_left_alone() {
        // 同轴三条且不相邻 → 无法确定谁配谁，不挪动
        let input = vec![
            d("0:00:01.00", "0:00:02.00", "你好", 1),
            d("0:00:03.00", "0:00:04.00", "中间", 2),
            d("0:00:01.00", "0:00:02.00", "Hello", 3),
            d("0:00:05.00", "0:00:06.00", "后面", 4),
            d("0:00:01.00", "0:00:02.00", "Hi", 5),
        ];
        let (out, _) = merge_bilingual(input);
        assert_eq!(out.len(), 5);
    }

    #[test]
    fn non_adjacent_digits_only_english_row_paired() {
        // 英文行只有数字和标点（如年份「1955.」）也算英文译文，应与中文配对合并
        let input = vec![
            d("0:26:00.01", "0:26:01.84", "1955年", 1),
            d("0:26:02.00", "0:26:03.00", "中间", 2),
            d("0:26:00.01", "0:26:01.84", "1955.", 3),
        ];
        let (out, _) = merge_bilingual(input);
        assert_eq!(out.len(), 2);
        assert_eq!(out[0].text, format!("1955年{SEPARATOR}1955."));
        assert_eq!(out[0].src_lines, vec![1, 3]);
    }

    #[test]
    fn non_adjacent_punct_only_row_not_paired() {
        // 只有标点、没有字母也没有数字的行不当作英文译文，不挪动
        let input = vec![
            d("0:00:01.00", "0:00:02.00", "你好", 1),
            d("0:00:03.00", "0:00:04.00", "中间", 2),
            d("0:00:01.00", "0:00:02.00", "...", 3),
        ];
        let (out, _) = merge_bilingual(input);
        assert_eq!(out.len(), 3);
    }

    #[test]
    fn font_name_with_chinese_does_not_hide_english_row() {
        // 英文行带中文字体名标签：仍应识别为英文并合并，不能把英文丢掉
        let input = vec![
            d("0:00:01.00", "0:00:02.00", "你好", 1),
            d("0:00:01.00", "0:00:02.00", "{\\fn微软雅黑}Hello", 2),
        ];
        let (out, _) = merge_bilingual(input);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].zh(), "你好");
        assert_eq!(out[0].en(), Some("{\\fn微软雅黑}Hello".to_string()));
    }

    #[test]
    fn bare_line_break_between_zh_and_en_becomes_separator() {
        let input = vec![ d("0:00:01.00", "0:00:02.00", "慢点好吗 迈克\\NHey slow down Mike", 1) ];
        let (out, _) = merge_bilingual(input);
        assert_eq!(out[0].text, format!("慢点好吗 迈克{SEPARATOR}Hey slow down Mike"));
    }

    #[test]
    fn bare_line_break_picks_the_zh_en_boundary() {
        // 中文\N中文\N英文 → 只在中英交界处换成标准分隔
        assert_eq!(
            unify_same_row_sep("第一行\\N第二行\\NEnglish line"),
            format!("第一行\\N第二行{SEPARATOR}English line"));
        // 中文\N英文\N英文 → 第一处就是交界
        assert_eq!(
            unify_same_row_sep("中文\\NEnglish one\\NEnglish two"),
            format!("中文{SEPARATOR}English one\\NEnglish two"));
    }

    #[test]
    fn bare_line_break_within_one_language_untouched() {
        for t in ["第一行\\N第二行", "Line one\\NLine two", "中文\\N123", "中文\\N123\\N456", "中文\\NEnglish\\N又是中文"] {
            assert_eq!(unify_same_row_sep(t), t);
        }
    }
}

#[cfg(test)]
mod cleanup_tests {
    use super::*;

    #[test]
    fn reset_tag_removed_from_output() {
        let ass = "[Events]\n\
Dialogue: 0,0:00:01.00,0:00:02.00,Default,,0,0,0,,爸爸\\N{\\fnArial\\fs30}Baba.{\\r}\n\
Dialogue: 0,0:00:03.00,0:00:04.00,Default,,0,0,0,,精灵语{\\r}\\N{\\fnCronos Pro Subhead\\fs14}I am Arwen.{\\r}\n";
        let (out, issues) = format_ass(ass, &[]);
        assert!(!out.contains("{\\r}"), "{out}");
        assert!(out.contains("爸爸\\N{\\fnArial\\fs30}Baba."));
        assert!(out.contains("精灵语\\N{\\fnArial\\fs30}I am Arwen."));
        assert!(!issues.iter().any(|i| i.kind == "异常字体"), "{issues:?}");
    }

    #[test]
    fn legacy_song_mark_replaced_in_both_languages() {
        let ass = "[Events]\n\
Dialogue: 0,0:00:01.00,0:00:02.00,Default,,0,0,0,,∮ 有一天当你想唱歌时 ∮\\N{\\fnArial\\fs30}∮ Someday when you sing ∮\n\
Dialogue: 0,0:00:03.00,0:00:04.00,Default,,0,0,0,,∮ 啦啦啦 ∮\n\
Dialogue: 0,0:00:05.00,0:00:06.00,Default,,0,0,0,,♪ 本来就是八分音符 ♪\n\
Dialogue: 0,0:00:07.00,0:00:08.00,Default,,0,0,0,,♫ 这个要人工改 ♫\n";
        let (out, issues) = format_ass(ass, &[]);
        assert!(!out.contains('∮'), "{out}");
        assert!(out.contains("♪ 有一天当你想唱歌时 ♪\\N{\\fnArial\\fs30}♪ Someday when you sing ♪\n"), "{out}");
        assert!(out.contains(",,♪ 啦啦啦 ♪\n"), "{out}");
        assert!(out.contains(",,♪ 本来就是八分音符 ♪\n"), "{out}");
        // 其它乐符不自动改，只提示这一行
        assert!(out.contains("♫ 这个要人工改 ♫"), "{out}");
        let song: Vec<&Issue> = issues.iter().filter(|i| i.kind == "非标准音乐符").collect();
        assert_eq!(song.len(), 1, "{issues:?}");
        assert_eq!(song[0].start, "0:00:07.00");
    }

    #[test]
    fn separated_blocks_merged_in_pipeline() {
        let ass = "[Events]\n\
Dialogue: 0,0:00:01.00,0:00:02.00,ENG,,0,0,0,,Once upon a time{\\r}\n\
Dialogue: 0,0:00:03.00,0:00:04.00,ENG,,0,0,0,,The end\n\
Dialogue: 0,0:00:01.00,0:00:02.00,CHS,,0,0,0,,从前\n\
Dialogue: 0,0:00:03.00,0:00:04.00,CHS,,0,0,0,,完\n";
        let (out, _) = format_ass(ass, &[]);
        assert_eq!(out.matches("Dialogue:").count(), 2, "{out}");
        assert!(out.contains("从前\\N{\\fnArial\\fs30}Once upon a time."));
        assert!(out.contains("完\\N{\\fnArial\\fs30}The end."));
    }

    #[test]
    fn style_types_in_pipeline() {
        let ass = "[Events]\n\
Dialogue: 0,0:00:01.00,0:00:02.00,Default,,0,0,0,,你好\\N{\\fnArial\\fs30}Hi\n\
Dialogue: 0,0:00:03.00,0:00:04.00,Default,,0,0,0,,普通台词\n\
Dialogue: 0,0:00:05.00,0:00:06.00,Default,,0,0,0,,《斯巴达克斯: 血与沙》\n\
Dialogue: 0,0:00:07.00,0:00:08.00,Default,,0,0,0,,（一把钥匙）\n\
Dialogue: 0,0:00:09.00,0:00:10.00,Default,,0,0,0,,是 《奇异恩典》啊\n\
Dialogue: 0,0:00:11.00,0:00:12.00,Default,,0,0,0,,[外语] 早上好\n\
Dialogue: 0,0:00:13.00,0:00:14.00,Default,,0,0,0,,中文\\N{\\fnArial\\fs30}\n\
Dialogue: 0,0:00:15.00,0:00:16.00,Default,,0,0,0,,(半角注释)\n";
        let (out, issues) = format_ass(ass, &[]);
        let styles: Vec<&str> = out.lines().filter(|l| l.starts_with("Dialogue:"))
            .map(|l| l.split(',').nth(3).unwrap()).collect();
        assert_eq!(styles, ["Bilingual", "Unilingual", "Title", "Note", "Unilingual", "Unilingual", "Unilingual", "Note"], "{out}");
        assert!(out.contains(",Title,0,0,0,0,,《斯巴达克斯：血与沙》\n"), "{out}");
        assert!(out.contains(",Note,0,0,0,0,,(一把钥匙)\n"), "{out}");
        assert!(out.contains(",Note,0,0,0,0,,(半角注释)\n"), "{out}");
        assert!(out.contains(",Unilingual,0,0,0,0,,是《奇异恩典》啊\n"), "{out}");
        assert!(out.contains(",Unilingual,0,0,0,0,,中文\n"), "{out}");
        assert!(!out.contains("Default"), "{out}");
        assert!(!issues.iter().any(|i| i.kind.starts_with("样式存疑")), "{issues:?}");
    }

    #[test]
    fn header_declares_four_styles() {
        let header = build_header();
        let style_lines: Vec<&str> = header.lines().filter(|l| l.starts_with("Style: ")).collect();
        let names: Vec<&str> = style_lines.iter()
            .map(|l| l["Style: ".len()..].split(',').next().unwrap()).collect();
        assert_eq!(names, ["Bilingual", "Unilingual", "Title", "Note"]);
        // Unilingual 与 Note 除名字外完全相同
        let params = |name: &str| style_lines.iter()
            .find(|l| l.starts_with(&format!("Style: {name},")))
            .map(|l| l.splitn(2, ',').nth(1).unwrap().to_string()).unwrap();
        assert_eq!(params("Unilingual"), params("Note"));
        assert_ne!(params("Bilingual"), params("Note"));
    }

    #[test]
    fn style_doubts_reported() {
        let ass = "[Events]\n\
Dialogue: 0,0:00:01.00,0:00:02.00,Default,,0,0,0,,《小红帽》\\N{\\fnArial\\fs30}Snow White?\n\
Dialogue: 0,0:00:03.00,0:00:04.00,Default,,0,0,0,,(枪声)(枪声)\n\
Dialogue: 0,0:00:05.00,0:00:06.00,Default,,0,0,0,,（两天没换的衬衫\n\
Dialogue: 0,0:00:07.00,0:00:08.00,Default,,0,0,0,,普通台词\n";
        let (out, issues) = format_ass(ass, &[]);
        let doubts: Vec<(&str, &str)> = issues.iter()
            .filter(|i| i.kind.starts_with("样式存疑"))
            .map(|i| (i.kind.as_str(), i.start.as_str())).collect();
        assert_eq!(doubts, [
            ("样式存疑(双语)", "0:00:01.00"),
            ("样式存疑(多段)", "0:00:03.00"),
            ("样式存疑(未闭合)", "0:00:05.00"),
        ]);
        // 存疑行不擅自改：双语标题保持 Bilingual，多段/未闭合保持 Unilingual
        assert!(out.contains("0:00:01.00,0:00:02.00,Bilingual,"), "{out}");
        assert!(out.contains("0:00:03.00,0:00:04.00,Unilingual,"), "{out}");
        assert!(out.contains("0:00:05.00,0:00:06.00,Unilingual,"), "{out}");
    }

    #[test]
    fn leftover_font_tag_flagged() {
        let ass = "[Events]\n\
Dialogue: 0,0:00:01.00,0:00:02.00,Default,,0,0,0,,{\\fn微软雅黑}你好\\N{\\fnArial\\fs30}Hello.\n\
Dialogue: 0,0:00:03.00,0:00:04.00,Default,,0,0,0,,再见\\N{\\fnArial\\fs30}Bye.\n\
Dialogue: 0,0:00:05.00,0:00:06.00,Default,,0,0,0,,一\\N{\\fnCronos Pro}One\\N{\\fnKaiTi\\fs20}two.\n";
        let (_, issues) = format_ass(ass, &[]);
        let fonts: Vec<&Issue> = issues.iter().filter(|i| i.kind == "异常字体").collect();
        assert_eq!(fonts.len(), 2, "{issues:?}");
        assert_eq!(fonts[0].start, "0:00:01.00");
        assert_eq!(fonts[1].start, "0:00:05.00");
    }

    #[test]
    fn standard_separator_is_not_abnormal_font() {
        assert!(!has_abnormal_font("你好\\N{\\fnArial\\fs30}Hello."));
        assert!(!has_abnormal_font("纯中文"));
        assert!(has_abnormal_font("{\\fnKaiTi}你好"));
        assert!(has_abnormal_font("你好\\N{\\fnArial\\fs30}{\\fnArial\\fs30}Hello."));
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

    #[test]
    fn suspicious_punct_covers_all_three_rules() {
        assert!(has_suspicious_punct("Hello,.world"));      // ,.
        assert!(has_suspicious_punct("两个  空格"));         // 连续两个空格
        assert!(has_suspicious_punct("word .,broken"));     // 异常的 .,
        assert!(!has_suspicious_punct("Washington D.C., is nice"));
        assert!(!has_suspicious_punct("正常的一句 话。"));
    }

    #[test]
    fn punct_issue_kind_has_no_suffix_in_pipeline() {
        // 英文段「5.,next」规整后是「5., next」：点号前是数字，属异常 `.,`，会留到最后被检查到。
        // 类型名就是「残留可疑标点」，不带「[.,]」之类的后缀；缩写和正常句子不报。
        let ass = "[Events]\n\
Dialogue: 0,0:00:01.00,0:00:02.00,Default,,0,0,0,,第五章\\N{\\fnArial\\fs30}Chapter 5.,next\n\
Dialogue: 0,0:00:03.00,0:00:04.00,Default,,0,0,0,,华盛顿\\N{\\fnArial\\fs30}Washington D.C., is nice\n\
Dialogue: 0,0:00:05.00,0:00:06.00,Default,,0,0,0,,很好\\N{\\fnArial\\fs30}All good.\n";
        let (_, issues) = format_ass(ass, &[]);
        let punct: Vec<(&str, &str)> = issues.iter()
            .filter(|i| i.kind.contains("标点"))
            .map(|i| (i.kind.as_str(), i.start.as_str())).collect();
        assert_eq!(punct, [("残留可疑标点", "0:00:01.00")], "{issues:?}");
    }
}
