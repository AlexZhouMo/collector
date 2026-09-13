# 漫画自动归档 实现计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 把工具箱「漫画标准化」升级为「漫画自动归档」：一键遍历配置好的漫画根，自动分卷、转 JPG、命名 `XX_YYY.jpg`、打包 `Vol_XX.zip` 写回漫画目录，附进度条与成功/跳过/失败结果列表。

**Architecture:** 后端分三层——`comic_pack.rs` 提取共享打包核心 `pack_images_to_zip`；`comic_archive.rs` 负责卷识别、按父目录分组、组内从 01 编号（组 ≥100 卷用 3 位）、逐卷调用核心并产出 `ArchiveReport`（成功/跳过/失败）；async command `archive_comics` 读 `comic_root`、`spawn_blocking` 执行并 emit 进度。前端把漫画面板重构为与字幕校准同构的"只读目录 + 开始归档 + 进度条 + 结果列表"，完成后重扫漫画库。

**Tech Stack:** Rust（image/zip/walkdir/std）、Tauri command + Emitter、vanilla-ts 前端。

---

### Task 1: 提取 comic_pack 共享打包核心

**Files:**
- Modify: `src-tauri/src/normalize/comic_pack.rs`

把 `pack_comic_dir` 中"给定已排序图片路径列表 + 命名规则 → 转 JPEG → 打 zip"的逻辑提取为共享函数 `pack_images_to_zip`，供归档模块复用。`pack_comic_dir` 改为调用它（保持旧行为与测试不变）。

- [ ] **Step 1: 写失败测试**

在 `comic_pack.rs` 的 `mod tests` 内新增（测试新函数直接可用，命名回调形式）：

```rust
    #[test]
    fn pack_images_to_zip_uses_namer() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("src");
        std::fs::create_dir_all(&dir).unwrap();
        for name in ["1.png", "2.png"] {
            let img = RgbImage::from_pixel(4, 4, Rgb([10, 20, 30]));
            img.save(dir.join(name)).unwrap();
        }
        let imgs = vec![dir.join("1.png"), dir.join("2.png")];
        let out = tmp.path().join("out.zip");
        // namer: 第 i 张（0-based）→ 文件名
        let n = pack_images_to_zip(&imgs, &out, |i| format!("01_{:03}.jpg", i + 1)).unwrap();
        assert_eq!(n, 2);
        let mut ar = zip::ZipArchive::new(File::open(&out).unwrap()).unwrap();
        let names: Vec<String> = (0..ar.len()).map(|i| ar.by_index(i).unwrap().name().to_string()).collect();
        assert!(names.contains(&"01_001.jpg".to_string()));
        assert!(names.contains(&"01_002.jpg".to_string()));
    }
```

- [ ] **Step 2: 运行测试验证失败**

Run: `cd src-tauri && cargo test --lib comic_pack`
Expected: 编译失败——`pack_images_to_zip` 未定义。

- [ ] **Step 3: 实现共享函数并让 pack_comic_dir 复用**

在 `comic_pack.rs` 新增函数（放在 `pack_comic_dir` 之前或之后均可）：

```rust
/// 把已排序的图片路径列表转 JPEG、按 namer(i) 命名、打包成 out_zip，返回打包页数。
/// namer 接收 0-based 序号，返回 zip 内文件名。图片列表应已按需排序、已排除冗余文件。
pub fn pack_images_to_zip(
    images: &[std::path::PathBuf],
    out_zip: &Path,
    namer: impl Fn(usize) -> String,
) -> AppResult<usize> {
    if images.is_empty() {
        return Err(AppError::Invalid("no images".into()));
    }
    if let Some(parent) = out_zip.parent() {
        std::fs::create_dir_all(parent).ok();
    }
    let f = File::create(out_zip)?;
    let mut zw = zip::ZipWriter::new(f);
    let opt = SimpleFileOptions::default();
    let mut n = 0;
    for (i, path) in images.iter().enumerate() {
        let img = image::open(path).map_err(|e| AppError::Other(e.to_string()))?;
        let mut buf = std::io::Cursor::new(Vec::new());
        img.to_rgb8()
            .write_to(&mut buf, image::ImageFormat::Jpeg)
            .map_err(|e| AppError::Other(e.to_string()))?;
        zw.start_file(namer(i), opt).map_err(|e| AppError::Other(e.to_string()))?;
        zw.write_all(buf.get_ref())?;
        n += 1;
    }
    zw.finish().map_err(|e| AppError::Other(e.to_string()))?;
    Ok(n)
}
```

把 `pack_comic_dir` 尾部"创建 zip → 循环转码写入 → finish"整段替换为收集排序后调用共享函数：

```rust
pub fn pack_comic_dir(dir: &Path, prefix: &str, out_zip: &Path) -> AppResult<usize> {
    let mut files: Vec<_> = std::fs::read_dir(dir)?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| {
            if junk::is_system_junk_path(p) {
                return false;
            }
            let l = p.to_string_lossy().to_lowercase();
            l.ends_with(".jpg") || l.ends_with(".jpeg") || l.ends_with(".png") || l.ends_with(".webp") || l.ends_with(".bmp")
        })
        .collect();
    files.sort_by(|a, b| {
        let ka = natural_key(a.file_name().and_then(|s| s.to_str()).unwrap_or(""));
        let kb = natural_key(b.file_name().and_then(|s| s.to_str()).unwrap_or(""));
        ka.cmp(&kb)
    });
    let prefix = prefix.to_string();
    pack_images_to_zip(&files, out_zip, move |i| format!("{prefix}_{:03}.jpg", i + 1))
}
```

- [ ] **Step 4: 运行测试验证通过**

Run: `cd src-tauri && cargo test --lib comic_pack`
Expected: 全部 PASS（含新增 `pack_images_to_zip_uses_namer` 与原有两个测试）。

- [ ] **Step 5: 提交**

```bash
git add src-tauri/src/normalize/comic_pack.rs
git commit -m "refactor(comic_pack): 提取共享打包核心 pack_images_to_zip

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>"
```

---

### Task 2: 卷识别与分组编号纯函数

**Files:**
- Create: `src-tauri/src/normalize/comic_archive.rs`
- Modify: `src-tauri/src/normalize/mod.rs`（加 `pub mod comic_archive;`）

先实现两个纯函数：`find_volume_dirs(root)` 找出所有直接含图片的叶子目录；`plan_volumes(vol_dirs)` 按父目录分组、组内自然序编号、决定卷号位数，产出待归档卷计划。

- [ ] **Step 1: 写失败测试**

创建 `src-tauri/src/normalize/comic_archive.rs`：

```rust
use crate::normalize::comic_pack::{natural_key, pack_images_to_zip};
use crate::util::junk;
use std::path::{Path, PathBuf};

/// 一卷的归档结果。
#[derive(Debug, Clone, serde::Serialize)]
pub struct ArchiveReport {
    pub manga: String,  // 漫画名（卷目录的父目录名）
    pub vol: String,    // 卷 zip 名，如 "Vol_01"
    pub status: String, // "成功" / "跳过(已存在)" / "失败(<原因>)"
    pub pages: usize,
}

/// 一个待归档卷：源图片目录 + 其漫画（父目录）+ 组内卷号 + 卷号位数。
#[derive(Debug, Clone, PartialEq)]
pub struct VolumePlan {
    pub dir: PathBuf,
    pub manga_dir: PathBuf,
    pub manga: String,
    pub index: usize, // 1-based 卷号
    pub width: usize, // 卷号位数（2 或 3）
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn touch_img(dir: &Path, name: &str) {
        fs::create_dir_all(dir).unwrap();
        // 写一个最小合法 PNG（1x1）；测试分组/编号无需真图，但用真 png 便于后续复用
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
        // 海贼王两卷（index 1,2），火影一卷（index 1）
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
        // 构造 100 个卷目录，断言 width=3
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
}
```

在 `src-tauri/src/normalize/mod.rs` 的模块声明处新增：

```rust
pub mod comic_archive;
```

（注意：`natural_key` 与 `pack_images_to_zip` 需可从 comic_archive 引用——`natural_key` 当前是 comic_pack 内私有 `fn`，本任务需把它改为 `pub fn natural_key`；`NatChunk` 枚举也需 `pub`。在 Step 3 一并处理。）

- [ ] **Step 2: 运行测试验证失败**

Run: `cd src-tauri && cargo test --lib comic_archive`
Expected: 编译失败——`natural_key`/`NatChunk` 私有不可见，或函数未实现。

- [ ] **Step 3: 放开 natural_key 可见性**

在 `comic_pack.rs` 把 `enum NatChunk` 改为 `pub enum NatChunk`，`fn natural_key` 改为 `pub fn natural_key`。（`pack_images_to_zip` 已在 Task 1 为 `pub`。）

- [ ] **Step 4: 运行测试验证通过**

Run: `cd src-tauri && cargo test --lib comic_archive`
Expected: 3 个测试 PASS。

- [ ] **Step 5: 提交**

```bash
git add src-tauri/src/normalize/comic_archive.rs src-tauri/src/normalize/mod.rs src-tauri/src/normalize/comic_pack.rs
git commit -m "feat(comic_archive): 卷识别与按漫画分组编号(组>=100卷用3位)

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>"
```

---

### Task 3: 归档执行 + async command + 进度

**Files:**
- Modify: `src-tauri/src/normalize/comic_archive.rs`

实现 `archive_comics(root, progress) -> Vec<ArchiveReport>`（逐卷排序图片、命名、幂等跳过已存在 zip、失败不中断、上报进度），以及 async command `archive_comics`（读 `comic_root`、`spawn_blocking`、emit `comic-archive-progress`）。

- [ ] **Step 1: 写失败测试**

在 `comic_archive.rs` 的 `mod tests` 内新增：

```rust
    #[test]
    fn archive_generates_vol_zip_and_reports() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        touch_img(&root.join("海贼王/第01卷"), "1.png");
        touch_img(&root.join("海贼王/第01卷"), "2.png");
        touch_img(&root.join("海贼王/第02卷"), "1.png");
        let reports = archive_comics(root, |_, _| {});
        // 两卷都成功
        assert_eq!(reports.len(), 2);
        assert!(reports.iter().all(|r| r.status == "成功"));
        // zip 写回漫画目录（海贼王/ 下）
        assert!(root.join("海贼王/Vol_01.zip").is_file());
        assert!(root.join("海贼王/Vol_02.zip").is_file());
        // Vol_01 内含 01_001.jpg / 01_002.jpg
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
        // 预先放一个已存在的 Vol_01.zip
        fs::write(root.join("A/Vol_01.zip"), b"OLD").unwrap();
        let reports = archive_comics(root, |_, _| {});
        assert_eq!(reports.len(), 1);
        assert!(reports[0].status.contains("跳过"));
        // 原 zip 未被覆盖
        assert_eq!(fs::read(root.join("A/Vol_01.zip")).unwrap(), b"OLD");
    }

    #[test]
    fn archive_failure_does_not_abort_batch() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        // 一卷含损坏"图片"（.png 扩展名但非图片字节）
        fs::create_dir_all(root.join("B/vol1")).unwrap();
        fs::write(root.join("B/vol1/bad.png"), b"not an image").unwrap();
        // 另一卷正常
        touch_img(&root.join("B/vol2"), "1.png");
        let reports = archive_comics(root, |_, _| {});
        assert_eq!(reports.len(), 2);
        let bad = reports.iter().find(|r| r.vol == "Vol_01").unwrap();
        assert!(bad.status.starts_with("失败"));
        let good = reports.iter().find(|r| r.vol == "Vol_02").unwrap();
        assert_eq!(good.status, "成功");
    }
```

- [ ] **Step 2: 运行测试验证失败**

Run: `cd src-tauri && cargo test --lib comic_archive`
Expected: 编译失败——`archive_comics` 未定义。

- [ ] **Step 3: 实现 archive_comics + command**

在 `comic_archive.rs` 顶部补充 use：

```rust
use crate::error::AppResult;
use std::io::Write; // for tests using fs::write is std::fs; no extra needed — 若未用可省
```

（若 `std::io::Write` 未被用到，请勿引入，避免未使用告警。）

新增归档执行函数：

```rust
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
        // 收集卷内图片，排序
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
```

> 命令名说明：函数名用 `archive_comics_cmd`，通过 `#[tauri::command]` 默认以函数名注册；前端 invoke 名为 `archive_comics_cmd`。（Task 4 的 ipc 与 handler 用此名。）

- [ ] **Step 4: 运行测试验证通过**

Run: `cd src-tauri && cargo test --lib comic_archive`
Expected: 6 个测试全 PASS（3 个来自 Task 2 + 3 个本任务）。

- [ ] **Step 5: 提交**

```bash
git add src-tauri/src/normalize/comic_archive.rs
git commit -m "feat(comic_archive): 归档执行(幂等跳过/失败不中断)+进度命令

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>"
```

---

### Task 4: 注册 command 与 ipc、移除旧 normalize_comic

**Files:**
- Modify: `src-tauri/src/lib.rs`（handler 注册）
- Modify: `src/lib/ipc.ts`

- [ ] **Step 1: 注册新 command、移除旧 command**

在 `src-tauri/src/lib.rs` 的 `generate_handler![...]` 中，把这一行：

```rust
            normalize::comic_pack::normalize_comic,
```

替换为：

```rust
            normalize::comic_archive::archive_comics_cmd,
```

- [ ] **Step 2: 更新 ipc.ts**

在 `src/lib/ipc.ts` 顶部接口区新增：

```ts
export interface ArchiveReport { manga: string; vol: string; status: string; pages: number; }
```

把 `api` 对象里这两行：

```ts
  normalizeComic: (dir: string, prefix: string, outZip: string) =>
    invoke<number>("normalize_comic", { dir, prefix, outZip }),
```

替换为：

```ts
  archiveComics: () => invoke<ArchiveReport[]>("archive_comics_cmd"),
```

- [ ] **Step 3: 移除旧后端 command 定义**

在 `src-tauri/src/normalize/comic_pack.rs` 中删除 `normalize_comic` 这个 `#[tauri::command]` 函数（`pack_comic_dir` 与其测试保留，因为 `pack_images_to_zip`/`natural_key` 仍被复用且 pack_comic_dir 的测试锚定核心）。

```rust
// 删除这段：
#[tauri::command(rename_all = "camelCase")]
pub fn normalize_comic(dir: String, prefix: String, out_zip: String) -> AppResult<usize> {
    pack_comic_dir(Path::new(&dir), &prefix, Path::new(&out_zip))
}
```

- [ ] **Step 4: 编译验证**

Run: `cd src-tauri && cargo build --lib 2>&1 | grep -E "error|warning: unused|never used" | head`
Expected: 无 error；`pack_comic_dir` 若因移除 command 变为"仅测试使用"，允许其为 `#[cfg(test)]` 场景保留——若出现 `pack_comic_dir is never used` 告警，给它加 `#[allow(dead_code)]` 或确认它仍被 pack_images_to_zip 测试链路引用（其测试在 comic_pack 内，属 lib test，不算 dead）。若确不再被任何非测试代码用到且报 dead_code，加 `#[cfg(test)]` 或 `#[allow(dead_code)]`。

- [ ] **Step 5: 全量测试 + 前端类型检查 + 提交**

```bash
cd src-tauri && cargo test --lib 2>&1 | tail -3
cd .. && npx tsc --noEmit
```
Expected: cargo 全 PASS；tsc 无输出（通过）。

```bash
git add src-tauri/src/lib.rs src/lib/ipc.ts src-tauri/src/normalize/comic_pack.rs
git commit -m "feat: 注册 archive_comics 命令与 ipc，移除旧 normalize_comic 入口

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>"
```

---

### Task 5: 前端面板重构为「漫画自动归档」

**Files:**
- Modify: `src/views/NormalizeView.ts`

把「漫画标准化」面板整体替换为「漫画自动归档」，移动到「字幕批量校准」面板**之后**，与另外两个工具同构（只读目录 + 开始归档 + 进度条 + 结果列表），完成后重扫漫画库。

- [ ] **Step 1: 替换 HTML 结构**

在 `NormalizeView.ts` 的 `el.innerHTML` 模板中：

(a) 删除顶部旧的「漫画标准化」`<div class="glass" style="padding:16px;margin-bottom:16px">...</div>` 整块（含 `#c-dir`/`#c-prefix`/`#c-out`/`#c-run`/`#c-report`）。

(b) 在「字幕批量校准」`setting-card` 的**闭合 `</div>` 之后**，追加新面板：

```html
    <div class="glass setting-card">
      <div class="setting-card-head"><span class="setting-card-title">漫画自动归档</span></div>
      <div class="setting-row">
        <span class="setting-label">漫画目录</span>
        <span id="comic-dir-p" class="setting-path">未配置</span>
      </div>
      <div class="setting-actions">
        <button class="btn-primary icon-text" id="comic-run" disabled>${icon("play", 15)}<span class="btn-label">开始归档</span></button>
      </div>
      <div id="comic-progress" style="display:none;margin-top:10px">
        <div style="height:6px;border-radius:4px;background:var(--glass);overflow:hidden">
          <div id="comic-progress-bar" style="height:100%;width:0%;background:var(--accent);transition:width .2s"></div>
        </div>
        <div id="comic-progress-text" style="font-size:12px;color:var(--text-dim);margin-top:4px"></div>
      </div>
      <div id="comic-report" style="margin-top:10px"></div>
    </div>
```

- [ ] **Step 2: 移除旧漫画逻辑、加载漫画目录**

删除 `NormalizeView` 函数体内旧漫画相关代码：`cDir`/`cOut` 变量、`pick` 里针对 `#c-dir`/`#c-out` 的绑定、`#c-run` 的 onclick 整段。`subIn`/`pick`（字幕用）保留。

在读取字幕输入目录附近，新增加载漫画目录并启用按钮：

```ts
  api.getRoot("comic").then((d) => {
    const p = el.querySelector("#comic-dir-p");
    const run = el.querySelector<HTMLButtonElement>("#comic-run");
    if (d) {
      if (p) p.textContent = d;
      if (run) run.disabled = false;
    } else {
      if (p) p.textContent = "请先在设置中配置漫画根目录";
    }
  });
```

- [ ] **Step 3: 实现开始归档逻辑（与字幕校准同构）**

新增 `#comic-run` 的 onclick（放在字幕 `#sub-run` onclick 之后）：

```ts
  el.querySelector<HTMLButtonElement>("#comic-run")!.onclick = async () => {
    const btn = el.querySelector<HTMLButtonElement>("#comic-run")!;
    const label = btn.querySelector<HTMLElement>(".btn-label")!;
    const prog = el.querySelector<HTMLElement>("#comic-progress")!;
    const bar = el.querySelector<HTMLElement>("#comic-progress-bar")!;
    const text = el.querySelector<HTMLElement>("#comic-progress-text")!;
    btn.disabled = true; label.textContent = "归档中…";
    el.querySelector("#comic-report")!.innerHTML = "";
    bar.style.width = "0%"; text.textContent = "准备中…（正在扫描目录）"; prog.style.display = "block";
    let unlistenC: (() => void) | null = null;
    try {
      unlistenC = await listen<{ done: number; total: number }>("comic-archive-progress", (e) => {
        const { done, total } = e.payload;
        const pct = total ? Math.round((done / total) * 100) : 0;
        bar.style.width = pct + "%";
        text.textContent = `已归档 ${done}/${total}`;
      });
      await new Promise<void>((r) => requestAnimationFrame(() => requestAnimationFrame(() => r())));
      const reports = await api.archiveComics();
      bar.style.width = "100%";
      const ok = reports.filter(r => r.status === "成功").length;
      const skip = reports.filter(r => r.status.includes("跳过")).length;
      const fail = reports.filter(r => r.status.startsWith("失败")).length;
      const box = el.querySelector("#comic-report")!;
      const badge = `<span class="sub-count-badge ${fail ? "has" : "none"}">成功 ${ok}・跳过 ${skip}・失败 ${fail}</span>`;
      let html = `<div class="sub-summary"><span>✓ 处理 ${reports.length} 卷</span>${badge}</div>`;
      // 按漫画分组
      const byManga = new Map<string, typeof reports>();
      reports.forEach(r => { const a = byManga.get(r.manga) ?? []; a.push(r); byManga.set(r.manga, a); });
      byManga.forEach((vols, manga) => {
        const rows = vols.map(v => {
          const color = v.status === "成功" ? "#8fdca0" : v.status.includes("跳过") ? "#9db8ff" : "#ff9b9b";
          const extra = v.status === "成功" ? `${v.pages} 页` : v.status;
          return `<div class="sub-issue"><span class="sub-kind" style="--k:${color}">${esc(v.vol)}</span><span class="sub-text">${esc(extra)}</span></div>`;
        }).join("");
        html += `<details class="sub-file" open><summary><span class="sub-fname">${esc(manga)}</span><span class="sub-badge">${vols.length}</span></summary><div class="sub-issues">${rows}</div></details>`;
      });
      box.innerHTML = html;
      await api.scanRoot("comic"); // 重扫漫画库，让新 zip 立即出现
    } catch (e) {
      alert("漫画自动归档失败：" + e);
    } finally {
      if (unlistenC) { unlistenC(); unlistenC = null; }
      btn.disabled = false; label.textContent = "开始归档";
    }
  };
```

- [ ] **Step 4: 类型检查**

Run: `npx tsc --noEmit`
Expected: 无输出（通过）。若 `pick` 因移除 `#c-dir`/`#c-out` 引用而变为未使用，删除 `pick` 定义；若 `save` import 不再使用，从第 1 行 import 移除 `save`。

- [ ] **Step 5: 提交**

```bash
git add src/views/NormalizeView.ts
git commit -m "feat(normalize): 漫画面板重构为「漫画自动归档」(遍历漫画根/进度条/结果列表/自动重扫)

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>"
```

---

## 自查

- **规格覆盖**：卷识别（Task 2 `find_volume_dirs`）、按漫画分组从 01 编号 + ≥100 卷 3 位（Task 2 `plan_volumes`）、转 JPEG/命名 `XX_YYY.jpg`/打 `Vol_XX.zip`/写回漫画目录（Task 1 核心 + Task 3 `archive_comics`）、幂等跳过并显式记录（Task 3）、单卷失败不中断（Task 3）、进度条 + 结果列表 + 目录只读文本 + 开始归档按钮 + 面板移到字幕下 + 完成重扫（Task 5）、旧入口移除（Task 4）——规格各条均有对应任务。
- **占位符扫描**：无 TBD/TODO；每个代码步骤给出完整代码。
- **类型/命名一致性**：`ArchiveReport{manga,vol,status,pages}` 在 Task 2 定义、Task 3 产出、Task 4 ipc 接口、Task 5 前端消费字段一致；后端 command 函数名 `archive_comics_cmd`，Task 4 handler 注册与 ipc invoke 名一致（`"archive_comics_cmd"`）；`pack_images_to_zip`(Task 1) / `natural_key` 放开 pub(Task 2) 在 Task 3 被引用；进度事件名 `comic-archive-progress` 在 Task 3 emit、Task 5 listen 一致。
