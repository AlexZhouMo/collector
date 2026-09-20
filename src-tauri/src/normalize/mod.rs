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
use tauri::Manager;
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
    let app_data = app
        .path()
        .app_data_dir()
        .map_err(|e| crate::error::AppError::Other(format!("app_data_dir: {e}")))?;
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
    let app_data = app
        .path()
        .app_data_dir()
        .map_err(|e| crate::error::AppError::Other(format!("app_data_dir: {e}")))?;
    Ok(app_data.join("subtitles").to_string_lossy().into_owned())
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
}
