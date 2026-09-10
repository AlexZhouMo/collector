pub mod subtitle;
pub mod subtitle_check;
pub mod comic_pack;
pub mod special_chars;
pub mod punct;

use crate::error::AppResult;
use serde::Serialize;
use std::path::Path;
use tauri::Manager;
use walkdir::WalkDir;

#[derive(Debug, Serialize)]
pub struct SubtitleReport {
    pub file: String,
    pub issues: Vec<subtitle_check::Issue>,
}

/// 递归处理目录下所有 .ass：标准化写入 out_dir（保持相对结构），并返回质检报告。
pub fn run_subtitle_normalize(
    in_dir: &Path,
    out_dir: &Path,
    char_map: &[(String, String)],
) -> AppResult<Vec<SubtitleReport>> {
    let mut reports = Vec::new();
    for entry in WalkDir::new(in_dir).into_iter().filter_map(|e| e.ok()) {
        let p = entry.path();
        if p.extension().and_then(|s| s.to_str()) != Some("ass") {
            continue;
        }
        let raw = std::fs::read_to_string(p)?;
        let formatted = subtitle::format_ass(&raw, char_map);
        let issues = subtitle_check::check(&formatted);
        let rel = p.strip_prefix(in_dir).unwrap_or(p);
        let dest = out_dir.join(rel);
        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&dest, &formatted)?;
        reports.push(SubtitleReport {
            file: rel.to_string_lossy().into_owned(),
            issues,
        });
    }
    Ok(reports)
}

#[tauri::command(rename_all = "camelCase")]
pub fn normalize_subtitles(app: tauri::AppHandle, in_dir: String) -> AppResult<Vec<SubtitleReport>> {
    let app_data = app
        .path()
        .app_data_dir()
        .map_err(|e| crate::error::AppError::Other(format!("app_data_dir: {e}")))?;
    let out_dir = app_data.join("subtitles");
    run_subtitle_normalize(Path::new(&in_dir), &out_dir, &[])
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
        let reports = run_subtitle_normalize(&indir, &outdir, &[]).unwrap();
        assert_eq!(reports.len(), 1);
        let out = fs::read_to_string(outdir.join("剧集/a.ass")).unwrap();
        assert!(out.contains("你好！"));
        assert!(out.contains("[V4+ Styles]"));
    }
}
