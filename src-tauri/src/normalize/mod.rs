pub mod subtitle;
pub mod encoding;
pub mod subtitle_check;
pub mod comic_pack;
pub mod comic_archive;
pub mod special_chars;
pub mod punct;
pub mod dialogue;
pub mod classify;

use crate::error::AppResult;
use serde::Serialize;
use std::path::Path;
use walkdir::WalkDir;
use crate::util::junk;

#[derive(Debug, Serialize)]
pub struct SubtitleReport {
    pub file: String,
    pub issues: Vec<subtitle_check::Issue>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ContextLine {
    pub line_no: usize,
    pub start: String,
    pub end: String,
    pub text: String,
    pub is_target: bool,
}

/// 从 .ass 文本收集：以 center_lines 覆盖范围为中心、上下各 radius 物理行内的所有 Dialogue 行。
/// line_no 为原文物理行号(1-based)。text 为原文 Text 字段(未规整)。
pub fn collect_context(content: &str, center_lines: &[usize], radius: usize) -> Vec<ContextLine> {
    use crate::normalize::subtitle::preprocess;
    let lo = center_lines.iter().copied().min().unwrap_or(1).saturating_sub(radius).max(1);
    let hi = center_lines.iter().copied().max().unwrap_or(1).saturating_add(radius);
    let mut out = Vec::new();
    for (idx, line) in preprocess(content).lines().enumerate() {
        let no = idx + 1;
        if no < lo || no > hi { continue; }
        if !line.starts_with("Dialogue:") { continue; }
        let rest = &line["Dialogue:".len()..];
        let parts: Vec<&str> = rest.splitn(10, ',').collect();
        if parts.len() < 10 { continue; }
        out.push(ContextLine {
            line_no: no,
            start: parts[1].trim().to_string(),
            end: parts[2].trim().to_string(),
            text: parts[9].to_string(),
            is_target: center_lines.contains(&no),
        });
    }
    out
}

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LineEdit {
    pub line_no: usize,
    pub start: String,
    pub end: String,
    pub text: String,
    #[serde(default)]
    pub deleted: bool,
}

/// 按物理行号替换 Dialogue 行的 Start/End/Text 字段，保留其余字段与所有非 Dialogue 行。
/// 输出以 \n 分隔（preprocess 已统一换行）。
pub fn apply_edits(content: &str, edits: &[LineEdit]) -> String {
    use crate::normalize::subtitle::preprocess;
    use std::collections::HashMap;
    let map: HashMap<usize, &LineEdit> = edits.iter().map(|e| (e.line_no, e)).collect();
    let processed = preprocess(content);
    let mut out_lines: Vec<String> = Vec::new();
    for (idx, line) in processed.lines().enumerate() {
        let no = idx + 1;
        match map.get(&no) {
            Some(e) if line.starts_with("Dialogue:") => {
                if e.deleted {
                    // 标记删除的 Dialogue 行：不写回，从输出移除
                    continue;
                }
                let rest = &line["Dialogue:".len()..];
                let parts: Vec<&str> = rest.splitn(10, ',').collect();
                if parts.len() == 10 {
                    // Layer,Start,End,Style,Name,MarginL,MarginR,MarginV,Effect,Text
                    out_lines.push(format!(
                        "Dialogue:{},{},{},{},{},{},{},{},{},{}",
                        parts[0], e.start, e.end, parts[3], parts[4],
                        parts[5], parts[6], parts[7], parts[8], e.text
                    ));
                } else {
                    out_lines.push(line.to_string());
                }
            }
            _ => out_lines.push(line.to_string()),
        }
    }
    let mut s = out_lines.join("\n");
    s.push('\n');
    s
}

/// 「疑似漏译」一键标记用的行首标签（含末尾空格）。
pub const FOREIGN_TAG: &str = "[外语] ";

/// 给一条字幕正文加上 FOREIGN_TAG。
/// - 对话式文本（以 `-` 开头、且有两个及以上对话破折号）：每一段各插一个，
///   并顺手把破折号后的空格规整成一个：`-你好 -再见` → `- [外语] 你好 - [外语] 再见`；
///   已带方括号标签的段、空段不动。
/// - 其它：插在句首；已以 `[外语]` 开头则原样返回。
///
/// 对话破折号的判定与校准管线一致：位于行首或空白之后的 `-`；另外也认紧跟在 `\N` 之后的
/// （`- 甲\N- 乙` 这种分两行写的对话）。词中的连字符不算。
fn insert_foreign_tag(text: &str) -> String {
    let tag = FOREIGN_TAG.trim_end();
    let body = text.trim_start();
    let chars: Vec<char> = body.chars().collect();
    let is_dash = |i: usize| {
        chars[i] == '-'
            && (i == 0
                || chars[i - 1].is_whitespace()
                || (i >= 2 && chars[i - 2] == '\\' && matches!(chars[i - 1], 'N' | 'n')))
    };
    let dashes: Vec<usize> = (0..chars.len()).filter(|&i| is_dash(i)).collect();
    if dashes.first() == Some(&0) && dashes.len() >= 2 {
        let mut out = String::with_capacity(body.len() + dashes.len() * FOREIGN_TAG.len());
        for (k, &at) in dashes.iter().enumerate() {
            let end = dashes.get(k + 1).copied().unwrap_or(chars.len());
            let seg: String = chars[at + 1..end].iter().collect();
            let seg = seg.trim_start();
            out.push_str("- ");
            if !seg.trim().is_empty() && !seg.starts_with(['[', '［', '【']) {
                out.push_str(FOREIGN_TAG);
            }
            out.push_str(seg);
        }
        return out;
    }
    if body.starts_with(tag) {
        text.to_string()
    } else {
        format!("{FOREIGN_TAG}{body}")
    }
}

/// 给指定物理行的 Dialogue 正文插入 FOREIGN_TAG（规则见 `insert_foreign_tag`），其余字段与其它行不动。
/// 目标行里没有任何 Dialogue 行（原文已变化）返回 None。
pub fn tag_foreign(content: &str, lines: &[usize]) -> Option<String> {
    let edits: Vec<LineEdit> = collect_context(content, lines, 0)
        .into_iter()
        .filter(|c| c.is_target)
        .map(|c| {
            let text = insert_foreign_tag(&c.text);
            LineEdit { line_no: c.line_no, start: c.start, end: c.end, text, deleted: false }
        })
        .collect();
    if edits.is_empty() {
        return None;
    }
    Some(apply_edits(content, &edits))
}

/// 递归处理目录下所有 .ass：标准化写入 out_dir（保持相对结构），并返回质检报告。
pub fn run_subtitle_normalize(
    in_dir: &Path,
    out_dir: &Path,
    char_map: &[(String, String)],
    mut progress: impl FnMut(usize, usize),
) -> AppResult<Vec<SubtitleReport>> {
    // 先收集所有 .ass 路径，得到总数
    let files: Vec<_> = WalkDir::new(in_dir)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| !junk::is_system_junk_path(e.path()))
        .filter(|e| e.path().extension().and_then(|s| s.to_str()) == Some("ass"))
        .map(|e| e.path().to_path_buf())
        .collect();
    let total = files.len();
    progress(0, total); // 扫描完成即刻上报总数，让前端进度条立即显示 0/total
    let mut reports = Vec::new();
    for (i, p) in files.iter().enumerate() {
        let p = p.as_path();
        let rel = p.strip_prefix(in_dir).unwrap_or(p);
        let raw = match encoding::read_subtitle(p) {
            Some(r) => r,
            None => {
                // 读取/解码失败：记一条 Issue，跳过该文件，不中断整批
                reports.push(SubtitleReport {
                    file: rel.to_string_lossy().into_owned(),
                    issues: vec![subtitle_check::Issue {
                        line: 0,
                        kind: "读取失败(编码无法识别)".into(),
                        text: String::new(),
                        start: String::new(),
                        end: String::new(),
                        src_lines: Vec::new(),
                    }],
                });
                progress(i + 1, total);
                continue;
            }
        };
        let (formatted, issues) = subtitle::format_ass(&raw, char_map);
        let dest = out_dir.join(rel);
        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&dest, &formatted)?;
        reports.push(SubtitleReport {
            file: rel.to_string_lossy().into_owned(),
            issues,
        });
        progress(i + 1, total);
    }
    Ok(reports)
}

#[tauri::command(rename_all = "camelCase")]
pub async fn normalize_subtitles(
    app: tauri::AppHandle,
    in_dir: String,
) -> AppResult<Vec<SubtitleReport>> {
    use tauri::Emitter;
    let app_data = crate::util::app_data_dir(&app)?;
    let out_dir = app_data.join("subtitles");
    let app2 = app.clone();
    let reports = tauri::async_runtime::spawn_blocking(move || -> AppResult<Vec<SubtitleReport>> {
        run_subtitle_normalize(Path::new(&in_dir), &out_dir, &[], move |done, total| {
            let _ = app2.emit(
                "subtitle-progress",
                serde_json::json!({ "done": done, "total": total }),
            );
        })
    })
    .await
    .map_err(|e| crate::error::AppError::Other(format!("join: {e}")))??;
    Ok(reports)
}

/// 返回当前系统下字幕库的实际存储目录（app_data_dir/subtitles）的绝对路径字符串。
/// 与 normalize_subtitles 的输出目录一致，供 UI 显示。
#[tauri::command]
pub fn subtitle_output_dir(app: tauri::AppHandle) -> AppResult<String> {
    Ok(ensure_subtitle_dir(&app)?.to_string_lossy().into_owned())
}

/// <app_data>/subtitles，不存在则创建（全新安装且尚未校准过字幕时 UI 显示的目录也真实存在）。
fn ensure_subtitle_dir(app: &tauri::AppHandle) -> AppResult<std::path::PathBuf> {
    let dir = crate::util::app_data_dir(app)?.join("subtitles");
    std::fs::create_dir_all(&dir)
        .map_err(|e| crate::error::AppError::Other(format!("创建字幕目录失败 {}: {e}", dir.display())))?;
    Ok(dir)
}

/// 在系统文件管理器中打开字幕输出目录：Windows 用资源管理器，macOS 用 Finder（open），其它平台 xdg-open。
/// AppData / ~/Library 默认隐藏，手动找不方便，故提供按钮直达。
#[tauri::command]
pub fn open_subtitle_output_dir(app: tauri::AppHandle) -> AppResult<()> {
    let dir = ensure_subtitle_dir(&app)?;
    #[cfg(target_os = "windows")]
    let prog = "explorer";
    #[cfg(target_os = "macos")]
    let prog = "open";
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    let prog = "xdg-open";
    // explorer.exe 即使成功也常返回非 0，只在无法启动时报错
    std::process::Command::new(prog)
        .arg(&dir)
        .spawn()
        .map_err(|e| crate::error::AppError::Other(format!("无法打开目录 {}: {e}", dir.display())))?;
    Ok(())
}

/// 读原始输入文件 in_dir/file，返回目标行±radius 范围内的 Dialogue 行。
#[tauri::command(rename_all = "camelCase")]
pub fn read_subtitle_context(
    in_dir: String,
    file: String,
    center_lines: Vec<usize>,
    radius: usize,
) -> AppResult<Vec<ContextLine>> {
    let path = Path::new(&in_dir).join(&file);
    let raw = encoding::read_subtitle(&path)
        .ok_or_else(|| crate::error::AppError::Other("原文无法读取或解码".into()))?;
    let lines = collect_context(&raw, &center_lines, radius);
    if lines.is_empty() {
        return Err(crate::error::AppError::Other("原文已变化，请重新校准".into()));
    }
    Ok(lines)
}

/// 把 edits 按物理行写回原始文件 in_dir/file，再对该文件重跑校验，返回新 issues。
#[tauri::command(rename_all = "camelCase")]
pub fn save_subtitle_edits(
    in_dir: String,
    file: String,
    edits: Vec<LineEdit>,
) -> AppResult<Vec<subtitle_check::Issue>> {
    let path = Path::new(&in_dir).join(&file);
    let raw = encoding::read_subtitle(&path)
        .ok_or_else(|| crate::error::AppError::Other("原文无法读取或解码".into()))?;
    let edited = apply_edits(&raw, &edits);
    std::fs::write(&path, edited.as_bytes())?;
    // 重校：对写回后的内容重跑 format_ass，取其 issues
    let (_, issues) = subtitle::format_ass(&edited, &[]);
    Ok(issues)
}

/// 「疑似漏译」一键标记：给原始文件 in_dir/file 的指定物理行正文行首插入 `[外语] `，
/// 写回后对该文件重跑校验，返回新 issues。
#[tauri::command(rename_all = "camelCase")]
pub fn tag_subtitle_foreign(
    in_dir: String,
    file: String,
    src_lines: Vec<usize>,
) -> AppResult<Vec<subtitle_check::Issue>> {
    let path = Path::new(&in_dir).join(&file);
    let raw = encoding::read_subtitle(&path)
        .ok_or_else(|| crate::error::AppError::Other("原文无法读取或解码".into()))?;
    let edited = tag_foreign(&raw, &src_lines)
        .ok_or_else(|| crate::error::AppError::Other("原文已变化，请重新校准".into()))?;
    std::fs::write(&path, edited.as_bytes())?;
    let (_, issues) = subtitle::format_ass(&edited, &[]);
    Ok(issues)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    #[test]
    fn normalizes_dir_and_reports() {
        let tmp = tempfile::tempdir().unwrap();
        let indir = tmp.path().join("in");
        let outdir = tmp.path().join("out");
        fs::create_dir_all(indir.join("剧集")).unwrap();
        fs::write(
            indir.join("剧集/a.ass"),
            "\u{feff}[Events]\r\nDialogue: 0,0:00:01.00,0:00:02.00,Default,,0,0,0,,你好!\\N{\\fnArial\\fs30}Hi!\r\n",
        )
        .unwrap();
        let reports = run_subtitle_normalize(&indir, &outdir, &[], |_, _| {}).unwrap();
        assert_eq!(reports.len(), 1);
        let out = fs::read_to_string(outdir.join("剧集/a.ass")).unwrap();
        assert!(out.contains("你好！"));
        assert!(out.contains("[V4+ Styles]"));
    }

    /// ♪ 从各种编码的原文读入、写出后必须还是 ♪（UTF-8 的 E2 99 AA），不能变成乱码或问号。
    #[test]
    fn song_mark_survives_every_input_encoding() {
        let tmp = tempfile::tempdir().unwrap();
        let indir = tmp.path().join("in");
        let outdir = tmp.path().join("out");
        fs::create_dir_all(&indir).unwrap();
        let ass = "[Events]\r\nDialogue: 0,0:00:01.00,0:00:02.00,Default,,0,0,0,,♪ 啦啦啦 ♪\r\nDialogue: 0,0:00:03.00,0:00:04.00,Default,,0,0,0,,∮ 旧符号 ∮\r\n";
        // UTF-8 无 BOM
        fs::write(indir.join("utf8.ass"), ass.as_bytes()).unwrap();
        // UTF-8 带 BOM
        fs::write(indir.join("utf8bom.ass"), [&[0xEF, 0xBB, 0xBF][..], ass.as_bytes()].concat()).unwrap();
        // UTF-16LE 带 BOM
        let utf16le: Vec<u8> = [0xFF, 0xFE].into_iter()
            .chain(ass.encode_utf16().flat_map(|u| u.to_le_bytes())).collect();
        fs::write(indir.join("utf16le.ass"), utf16le).unwrap();
        // UTF-16BE 带 BOM
        let utf16be: Vec<u8> = [0xFE, 0xFF].into_iter()
            .chain(ass.encode_utf16().flat_map(|u| u.to_be_bytes())).collect();
        fs::write(indir.join("utf16be.ass"), utf16be).unwrap();

        let reports = run_subtitle_normalize(&indir, &outdir, &[], |_, _| {}).unwrap();
        assert_eq!(reports.len(), 4);
        for name in ["utf8.ass", "utf8bom.ass", "utf16le.ass", "utf16be.ass"] {
            let bytes = fs::read(outdir.join(name)).unwrap();
            let out = String::from_utf8(bytes.clone()).unwrap_or_else(|_| panic!("{name}: 输出不是合法 UTF-8"));
            assert!(out.contains(",,♪ 啦啦啦 ♪\n"), "{name}: {out}");
            assert!(out.contains(",,♪ 旧符号 ♪\n"), "{name}: {out}");
            assert!(!out.contains('∮') && !out.contains('\u{fffd}') && !out.contains('?'), "{name}: {out}");
            // ♪ 的 UTF-8 字节序列逐字节确认，每个文件 4 个
            let needle = [0xE2u8, 0x99, 0xAA];
            assert_eq!(bytes.windows(3).filter(|w| *w == needle).count(), 4, "{name}");
        }
        // 编辑器保存路径：写回原文件后仍是 ♪
        let issues = save_subtitle_edits(
            indir.to_string_lossy().into_owned(), "utf16le.ass".into(),
            vec![LineEdit { line_no: 2, start: "0:00:01.00".into(), end: "0:00:02.00".into(), text: "♪ 改过 ♪".into(), deleted: false }],
        ).unwrap();
        assert!(!issues.iter().any(|i| i.kind == "非标准音乐符"), "{issues:?}");
        let saved = encoding::read_subtitle(&indir.join("utf16le.ass")).unwrap();
        assert!(saved.contains(",,♪ 改过 ♪\n"), "{saved}");
    }

    #[test]
    fn skips_system_junk_files() {
        let tmp = tempfile::tempdir().unwrap();
        let indir = tmp.path().join("in");
        let outdir = tmp.path().join("out");
        fs::create_dir_all(&indir).unwrap();
        fs::write(
            indir.join("a.ass"),
            "\u{feff}[Events]\r\nDialogue: 0,0:00:01.00,0:00:02.00,Default,,0,0,0,,你好\r\n",
        ).unwrap();
        fs::write(indir.join(".DS_Store"), b"junk").unwrap();
        fs::write(indir.join("Thumbs.db"), b"junk").unwrap();
        let reports = run_subtitle_normalize(&indir, &outdir, &[], |_, _| {}).unwrap();
        assert_eq!(reports.len(), 1); // 只处理 a.ass
        assert_eq!(reports[0].file, "a.ass");
    }

    #[test]
    fn context_returns_target_and_neighbors() {
        let ass = "[Events]\n\
Dialogue: 0,0:00:01.00,0:00:02.00,Default,,0,0,0,,A\n\
Dialogue: 0,0:00:03.00,0:00:04.00,Default,,0,0,0,,B\n\
Dialogue: 0,0:00:05.00,0:00:06.00,Default,,0,0,0,,C\n";
        // 目标物理行 3（B），radius 1 → 覆盖物理行 2..=4 内的 Dialogue = A,B,C
        let lines = collect_context(ass, &[3], 1);
        assert_eq!(lines.len(), 3);
        assert_eq!(lines[1].text, "B");
        assert!(lines[1].is_target);
        assert!(!lines[0].is_target);
        assert_eq!(lines[0].line_no, 2);
        assert_eq!(lines[1].line_no, 3);
    }

    #[test]
    fn apply_edits_replaces_only_time_and_text() {
        let ass = "[Events]\n\
Dialogue: 0,0:00:01.00,0:00:02.00,Title,NAME,5,6,7,fx,你好\n\
Dialogue: 0,0:00:03.00,0:00:04.00,Default,,0,0,0,,世界\n";
        let edits = vec![LineEdit {
            line_no: 2,
            start: "0:00:01.50".into(),
            end: "0:00:02.50".into(),
            text: "您好".into(),
            deleted: false,
        }];
        let out = apply_edits(ass, &edits);
        // 第2行时间与正文改，Style=Title/NAME/margins/fx 保留
        assert!(out.contains("Dialogue: 0,0:00:01.50,0:00:02.50,Title,NAME,5,6,7,fx,您好"));
        // 第3行完全不动
        assert!(out.contains("Dialogue: 0,0:00:03.00,0:00:04.00,Default,,0,0,0,,世界"));
        // 头部保留
        assert!(out.starts_with("[Events]"));
    }

    #[test]
    fn tag_foreign_prepends_tag_to_target_lines_only() {
        let ass = "[Events]\n\
Dialogue: 0,0:00:01.00,0:00:02.00,Title,NAME,5,6,7,fx,你好\n\
Dialogue: 0,0:00:03.00,0:00:04.00,Default,,0,0,0,,世界\n\
Dialogue: 0,0:00:05.00,0:00:06.00,Default,,0,0,0,,[外语] 已标记\n";
        let out = tag_foreign(ass, &[2, 4]).unwrap();
        // 目标行：正文行首加「[外语] 」，其余字段保留
        assert!(out.contains("Dialogue: 0,0:00:01.00,0:00:02.00,Title,NAME,5,6,7,fx,[外语] 你好\n"), "{out}");
        // 非目标行（夹在目标行之间）不动
        assert!(out.contains(",,世界\n"), "{out}");
        // 已带标签的行不重复插入
        assert!(out.contains(",,[外语] 已标记\n"), "{out}");
        assert!(!out.contains("[外语] [外语]"), "{out}");
    }

    #[test]
    fn insert_foreign_tag_per_segment_for_dash_dialogue() {
        // 对话式：每段各插一个
        assert_eq!(insert_foreign_tag("- 你们这是要去冰湖吗？ - 对"), "- [外语] 你们这是要去冰湖吗？ - [外语] 对");
        // 破折号后没空格的写法顺手规整
        assert_eq!(insert_foreign_tag("-你好 -再见"), "- [外语] 你好 - [外语] 再见");
        // 三段、以及用 \N 分两行写的对话
        assert_eq!(insert_foreign_tag("- 甲 - 乙 - 丙"), "- [外语] 甲 - [外语] 乙 - [外语] 丙");
        assert_eq!(insert_foreign_tag("- 甲\\N- 乙"), "- [外语] 甲\\N- [外语] 乙");
        // 已带方括号标签的段不重复插
        assert_eq!(insert_foreign_tag("- [外语] 甲 - 乙"), "- [外语] 甲 - [外语] 乙");
        assert_eq!(insert_foreign_tag("- [日语] 甲 - 乙"), "- [日语] 甲 - [外语] 乙");
    }

    #[test]
    fn insert_foreign_tag_at_front_otherwise() {
        assert_eq!(insert_foreign_tag("早上好"), "[外语] 早上好");
        assert_eq!(insert_foreign_tag("  早上好"), "[外语] 早上好");
        assert_eq!(insert_foreign_tag("[外语] 早上好"), "[外语] 早上好");
        // 只有一个破折号、或破折号不在句首：不算对话式，插在句首
        assert_eq!(insert_foreign_tag("- 早上好"), "[外语] - 早上好");
        assert_eq!(insert_foreign_tag("你好 - 再见"), "[外语] 你好 - 再见");
        // 词中的连字符不是对话破折号
        assert_eq!(insert_foreign_tag("- 这是T-800型"), "[外语] - 这是T-800型");
    }

    /// 对话式裸中文行：一键标记后每段都带标签，重校不再报漏译。
    #[test]
    fn tag_subtitle_foreign_clears_dash_dialogue_issue() {
        let tmp = tempfile::tempdir().unwrap();
        let mut ass = String::from("[Events]\n");
        for i in 0..5 {
            ass.push_str(&format!(
                "Dialogue: 0,0:00:{:02}.00,0:00:{:02}.50,Default,,0,0,0,,你好\\N{{\\fnArial\\fs30}}Hi\n", i * 2, i * 2));
        }
        ass.push_str("Dialogue: 0,0:00:20.00,0:00:21.00,Default,,0,0,0,,- 你们这是要去冰湖吗？ - 对\n"); // 物理第 7 行
        fs::write(tmp.path().join("a.ass"), &ass).unwrap();
        let (_, before) = subtitle::format_ass(&ass, &[]);
        assert!(before.iter().any(|i| i.kind == "疑似漏译" && i.src_lines == vec![7]), "{before:?}");

        let after = tag_subtitle_foreign(
            tmp.path().to_string_lossy().into_owned(), "a.ass".into(), vec![7],
        ).unwrap();
        assert!(!after.iter().any(|i| i.kind.contains("漏译")), "{after:?}");
        let saved = fs::read_to_string(tmp.path().join("a.ass")).unwrap();
        assert!(saved.contains(",,- [外语] 你们这是要去冰湖吗？ - [外语] 对\n"), "{saved}");
    }

    #[test]
    fn tag_foreign_none_when_target_is_not_dialogue() {
        let ass = "[Events]\nDialogue: 0,0:00:01.00,0:00:02.00,Default,,0,0,0,,你好\n";
        assert!(tag_foreign(ass, &[1]).is_none());  // 第 1 行是 [Events]
        assert!(tag_foreign(ass, &[9]).is_none());  // 越界
    }

    /// 双语文件里的裸中文行报「疑似漏译」；一键标记后写回原文，重校该提示消失。
    #[test]
    fn tag_subtitle_foreign_clears_the_issue() {
        let tmp = tempfile::tempdir().unwrap();
        let mut ass = String::from("[Events]\n");
        for i in 0..5 {
            ass.push_str(&format!(
                "Dialogue: 0,0:00:{:02}.00,0:00:{:02}.50,Default,,0,0,0,,你好\\N{{\\fnArial\\fs30}}Hi\n", i * 2, i * 2));
        }
        ass.push_str("Dialogue: 0,0:00:20.00,0:00:21.00,Default,,0,0,0,,早上好\n"); // 物理第 7 行
        fs::write(tmp.path().join("a.ass"), &ass).unwrap();
        let (_, before) = subtitle::format_ass(&ass, &[]);
        let miss = before.iter().find(|i| i.kind == "疑似漏译").expect("应报疑似漏译");
        assert_eq!(miss.src_lines, vec![7]);

        let after = tag_subtitle_foreign(
            tmp.path().to_string_lossy().into_owned(), "a.ass".into(), miss.src_lines.clone(),
        ).unwrap();
        assert!(!after.iter().any(|i| i.kind.contains("漏译")), "{after:?}");
        let saved = fs::read_to_string(tmp.path().join("a.ass")).unwrap();
        assert!(saved.contains(",,[外语] 早上好\n"), "{saved}");
    }

    #[test]
    fn apply_edits_deletes_marked_line() {
        let ass = "[Events]\n\
Dialogue: 0,0:00:01.00,0:00:02.00,Default,,0,0,0,,第一行\n\
Dialogue: 0,0:00:03.00,0:00:04.00,Default,,0,0,0,,第二行\n\
Dialogue: 0,0:00:05.00,0:00:06.00,Default,,0,0,0,,第三行\n";
        // 删除物理行 3（第二行）
        let edits = vec![LineEdit {
            line_no: 3, start: String::new(), end: String::new(), text: String::new(), deleted: true,
        }];
        let out = apply_edits(ass, &edits);
        assert!(!out.contains("第二行"));   // 被删
        assert!(out.contains("第一行"));    // 保留
        assert!(out.contains("第三行"));    // 保留
    }
}
