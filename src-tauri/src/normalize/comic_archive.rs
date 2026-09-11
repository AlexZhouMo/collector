use crate::error::AppResult;
use crate::normalize::comic_pack::{natural_key, pack_images_to_zip};
use crate::util::junk;
use std::path::{Path, PathBuf};

/// 一卷的归档结果。
#[derive(Debug, Clone, serde::Serialize)]
pub struct ArchiveReport {
    pub manga: String,
    pub vol: String,
    pub status: String,
    pub pages: usize,
}

/// 一个待归档卷：源图片目录 + 其漫画（父目录）+ 组内卷号 + 卷号位数。
#[derive(Debug, Clone, PartialEq)]
pub struct VolumePlan {
    pub dir: PathBuf,
    pub manga_dir: PathBuf,
    pub manga: String,
    pub index: usize,
    pub width: usize,
}

const IMG_EXTS: &[&str] = &["jpg", "jpeg", "png", "webp", "bmp"];

/// 目录是否直接包含至少一张图片（排除系统冗余文件）。
fn dir_has_images(dir: &Path) -> bool {
    let rd = match std::fs::read_dir(dir) {
        Ok(r) => r,
        Err(_) => return false,
    };
    for e in rd.filter_map(|e| e.ok()) {
        let p = e.path();
        if !p.is_file() || junk::is_system_junk_path(&p) {
            continue;
        }
        if let Some(ext) = p.extension().and_then(|s| s.to_str()) {
            if IMG_EXTS.contains(&ext.to_lowercase().as_str()) {
                return true;
            }
        }
    }
    false
}

/// 递归找出所有"直接含图片"的目录（含 root 自身若它直接含图片）。
pub fn find_volume_dirs(root: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
        if dir_has_images(dir) {
            out.push(dir.to_path_buf());
        }
        if let Ok(rd) = std::fs::read_dir(dir) {
            let mut subs: Vec<PathBuf> = rd
                .filter_map(|e| e.ok().map(|e| e.path()))
                .filter(|p| p.is_dir() && !junk::is_system_junk_path(p))
                .collect();
            subs.sort();
            for s in subs {
                walk(&s, out);
            }
        }
    }
    walk(root, &mut out);
    out
}

/// 把卷目录按父目录分组、组内按目录名自然序编号、按组卷数决定位数，产出计划。
pub fn plan_volumes(vol_dirs: &[PathBuf]) -> Vec<VolumePlan> {
    use std::collections::BTreeMap;
    let mut groups: BTreeMap<PathBuf, Vec<PathBuf>> = BTreeMap::new();
    for d in vol_dirs {
        let parent = d.parent().map(|p| p.to_path_buf()).unwrap_or_else(|| d.clone());
        groups.entry(parent).or_default().push(d.clone());
    }
    let mut plans = Vec::new();
    for (manga_dir, mut dirs) in groups {
        dirs.sort_by(|a, b| {
            let ka = natural_key(a.file_name().and_then(|s| s.to_str()).unwrap_or(""));
            let kb = natural_key(b.file_name().and_then(|s| s.to_str()).unwrap_or(""));
            ka.cmp(&kb)
        });
        let width = if dirs.len() >= 100 { 3 } else { 2 };
        let manga = manga_dir
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_string();
        for (i, dir) in dirs.into_iter().enumerate() {
            plans.push(VolumePlan {
                dir,
                manga_dir: manga_dir.clone(),
                manga: manga.clone(),
                index: i + 1,
                width,
            });
        }
    }
    plans
}

/// 遍历漫画根，识别并归档所有卷。progress(done, total) 逐卷上报。
/// 幂等：目标 Vol_XX.zip 已存在则跳过。单卷失败只记该卷，不中断整批。
pub fn archive_comics(
    root: &Path,
    mut progress: impl FnMut(usize, usize),
) -> Vec<ArchiveReport> {
    let vol_dirs = find_volume_dirs(root);
    let plans = plan_volumes(&vol_dirs);
    let total = plans.len();
    progress(0, total);
    let mut reports = Vec::new();
    for (i, plan) in plans.iter().enumerate() {
        let vol_name = format!("Vol_{:0width$}", plan.index, width = plan.width);
        let out_zip = plan.manga_dir.join(format!("{vol_name}.zip"));
        if out_zip.exists() {
            reports.push(ArchiveReport {
                manga: plan.manga.clone(),
                vol: vol_name,
                status: "跳过(已存在)".into(),
                pages: 0,
            });
            progress(i + 1, total);
            continue;
        }
        let mut imgs: Vec<PathBuf> = match std::fs::read_dir(&plan.dir) {
            Ok(rd) => rd
                .filter_map(|e| e.ok().map(|e| e.path()))
                .filter(|p| {
                    if p.is_file() && !junk::is_system_junk_path(p) {
                        if let Some(ext) = p.extension().and_then(|s| s.to_str()) {
                            return IMG_EXTS.contains(&ext.to_lowercase().as_str());
                        }
                    }
                    false
                })
                .collect(),
            Err(_) => Vec::new(),
        };
        imgs.sort_by(|a, b| {
            let ka = natural_key(a.file_name().and_then(|s| s.to_str()).unwrap_or(""));
            let kb = natural_key(b.file_name().and_then(|s| s.to_str()).unwrap_or(""));
            ka.cmp(&kb)
        });
        let vol_prefix = format!("{:0width$}", plan.index, width = plan.width);
        let status_pages = pack_images_to_zip(&imgs, &out_zip, move |p| {
            format!("{vol_prefix}_{:03}.jpg", p + 1)
        });
        let report = match status_pages {
            Ok(n) => ArchiveReport {
                manga: plan.manga.clone(),
                vol: vol_name,
                status: "成功".into(),
                pages: n,
            },
            Err(e) => ArchiveReport {
                manga: plan.manga.clone(),
                vol: vol_name,
                status: format!("失败({e})"),
                pages: 0,
            },
        };
        reports.push(report);
        progress(i + 1, total);
    }
    reports
}

/// 遍历配置的漫画根目录、自动归档。emit "comic-archive-progress" {done,total}。
#[tauri::command]
pub async fn archive_comics_cmd(
    app: tauri::AppHandle,
    db: tauri::State<'_, crate::db::Db>,
) -> AppResult<Vec<ArchiveReport>> {
    use tauri::Emitter;
    let root = crate::settings::get(&db, "comic_root")?
        .ok_or_else(|| crate::error::AppError::Invalid("comic root not set".into()))?;
    let app2 = app.clone();
    let reports = tauri::async_runtime::spawn_blocking(move || {
        archive_comics(Path::new(&root), move |done, total| {
            let _ = app2.emit(
                "comic-archive-progress",
                serde_json::json!({ "done": done, "total": total }),
            );
        })
    })
    .await
    .map_err(|e| crate::error::AppError::Other(format!("join: {e}")))?;
    Ok(reports)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn touch_img(dir: &Path, name: &str) {
        fs::create_dir_all(dir).unwrap();
        use image::{RgbImage, Rgb};
        RgbImage::from_pixel(2, 2, Rgb([1, 2, 3])).save(dir.join(name)).unwrap();
    }

    #[test]
    fn find_leaf_image_dirs() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        touch_img(&root.join("海贼王/第01卷"), "a.png");
        touch_img(&root.join("海贼王/第02卷"), "a.png");
        touch_img(&root.join("火影/单卷"), "a.png");
        let mut dirs = find_volume_dirs(root);
        dirs.sort();
        assert_eq!(dirs.len(), 3);
    }

    #[test]
    fn group_and_number_per_manga() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        touch_img(&root.join("海贼王/第01卷"), "a.png");
        touch_img(&root.join("海贼王/第02卷"), "a.png");
        touch_img(&root.join("火影/vol1"), "a.png");
        let dirs = find_volume_dirs(root);
        let plans = plan_volumes(&dirs);
        let op: Vec<_> = plans.iter().filter(|p| p.manga == "海贼王").collect();
        assert_eq!(op.len(), 2);
        assert_eq!(op[0].index, 1);
        assert_eq!(op[1].index, 2);
        assert_eq!(op[0].width, 2);
        let np: Vec<_> = plans.iter().filter(|p| p.manga == "火影").collect();
        assert_eq!(np.len(), 1);
        assert_eq!(np[0].index, 1);
    }

    #[test]
    fn width_is_3_when_group_has_100_plus() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("长篇");
        for i in 1..=100 {
            touch_img(&root.join(format!("{i:04}")), "a.png");
        }
        let dirs = find_volume_dirs(tmp.path());
        let plans = plan_volumes(&dirs);
        assert!(plans.iter().all(|p| p.width == 3));
        assert_eq!(plans.len(), 100);
    }

    #[test]
    fn archive_generates_vol_zip_and_reports() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        touch_img(&root.join("海贼王/第01卷"), "1.png");
        touch_img(&root.join("海贼王/第01卷"), "2.png");
        touch_img(&root.join("海贼王/第02卷"), "1.png");
        let reports = archive_comics(root, |_, _| {});
        assert_eq!(reports.len(), 2);
        assert!(reports.iter().all(|r| r.status == "成功"));
        assert!(root.join("海贼王/Vol_01.zip").is_file());
        assert!(root.join("海贼王/Vol_02.zip").is_file());
        let mut ar = zip::ZipArchive::new(std::fs::File::open(root.join("海贼王/Vol_01.zip")).unwrap()).unwrap();
        let names: Vec<String> = (0..ar.len()).map(|i| ar.by_index(i).unwrap().name().to_string()).collect();
        assert!(names.contains(&"01_001.jpg".to_string()));
        assert!(names.contains(&"01_002.jpg".to_string()));
    }

    #[test]
    fn archive_skips_existing_zip() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        touch_img(&root.join("A/vol1"), "1.png");
        fs::write(root.join("A/Vol_01.zip"), b"OLD").unwrap();
        let reports = archive_comics(root, |_, _| {});
        assert_eq!(reports.len(), 1);
        assert!(reports[0].status.contains("跳过"));
        assert_eq!(fs::read(root.join("A/Vol_01.zip")).unwrap(), b"OLD");
    }

    #[test]
    fn archive_failure_does_not_abort_batch() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        fs::create_dir_all(root.join("B/vol1")).unwrap();
        fs::write(root.join("B/vol1/bad.png"), b"not an image").unwrap();
        touch_img(&root.join("B/vol2"), "1.png");
        let reports = archive_comics(root, |_, _| {});
        assert_eq!(reports.len(), 2);
        let bad = reports.iter().find(|r| r.vol == "Vol_01").unwrap();
        assert!(bad.status.starts_with("失败"));
        let good = reports.iter().find(|r| r.vol == "Vol_02").unwrap();
        assert_eq!(good.status, "成功");
    }
}
