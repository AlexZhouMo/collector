# 归档进度三级细节显示 实现计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 归档进度条按图片总数平滑推进（卷内逐图实时刷新），文本显示"正在：{漫画} {卷}｜图片 {done}/{total}"。

**Architecture:** `pack_images_to_zip` 加每图回调 `on_image`；`archive_comics` 预扫总图片数、用共享进度状态 `ArchiveShared`（`AtomicUsize done_images` + `Mutex<(manga,vol)>` + total 常量），`on_image` 原子递增 done_images、进卷时更新 manga/vol；command 层起后台轮询线程每 ~80ms 读共享状态 emit，实现卷内逐图实时刷新；前端 listen 渲染。

**Tech Stack:** Rust（rayon/Arc/AtomicUsize/Mutex/thread）、Tauri Emitter、vanilla-ts。

---

### Task 1: pack_images_to_zip 增加 on_image 回调

**Files:**
- Modify: `src-tauri/src/normalize/comic_pack.rs`

- [ ] **Step 1: 写失败测试**

在 `comic_pack.rs` 的 `mod tests` 内新增：

```rust
    #[test]
    fn on_image_called_once_per_image() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("src");
        std::fs::create_dir_all(&dir).unwrap();
        let imgs: Vec<_> = (0..5).map(|k| {
            let p = dir.join(format!("i{k}.png"));
            RgbImage::from_pixel(4, 4, Rgb([10, 20, 30])).save(&p).unwrap();
            p
        }).collect();
        let out = tmp.path().join("out.zip");
        let cnt = AtomicUsize::new(0);
        let n = pack_images_to_zip(&imgs, &out, |i| format!("p_{:03}.jpg", i + 1),
            || { cnt.fetch_add(1, Ordering::Relaxed); }).unwrap();
        assert_eq!(n, 5);
        assert_eq!(cnt.load(Ordering::Relaxed), 5);
    }
```

同时**更新现有 3 个调用点**：给 `pack_images_to_zip_uses_namer`、`parallel_encoding_preserves_page_order`、`parallel_propagates_decode_error` 里的 `pack_images_to_zip(...)` 调用末尾加 `, || {}`。

- [ ] **Step 2: 运行测试验证失败**

Run: `cd src-tauri && cargo test --lib comic_pack`
Expected: 编译失败——参数数量不符。

- [ ] **Step 3: 加 on_image 参数**

```rust
pub fn pack_images_to_zip(
    images: &[std::path::PathBuf],
    out_zip: &Path,
    namer: impl Fn(usize) -> String,
    on_image: impl Fn() + Sync,
) -> AppResult<usize> {
    if images.is_empty() {
        return Err(AppError::Invalid("no images".into()));
    }
    if let Some(parent) = out_zip.parent() {
        std::fs::create_dir_all(parent).ok();
    }
    let mut encoded: Vec<(usize, AppResult<Vec<u8>>)> = images
        .par_iter()
        .enumerate()
        .map(|(i, path)| {
            let r = encode_one_jpeg(path);
            on_image();
            (i, r)
        })
        .collect();
    encoded.sort_by_key(|(i, _)| *i);
    let f = File::create(out_zip)?;
    let mut zw = zip::ZipWriter::new(f);
    let opt = SimpleFileOptions::default();
    let mut n = 0;
    for (i, res) in encoded {
        let bytes = res?;
        zw.start_file(namer(i), opt).map_err(|e| AppError::Other(e.to_string()))?;
        zw.write_all(&bytes)?;
        n += 1;
    }
    zw.finish().map_err(|e| AppError::Other(e.to_string()))?;
    Ok(n)
}
```

`pack_comic_dir` 里调用末尾加 `, || {}`。

- [ ] **Step 4: 运行测试验证通过**

Run: `cd src-tauri && cargo test --lib comic_pack`
Expected: 全 PASS。

- [ ] **Step 5: 提交**

```bash
git add src-tauri/src/normalize/comic_pack.rs
git commit -m "feat(comic_pack): pack_images_to_zip 增加每图完成回调 on_image

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>"
```

---

### Task 2: archive_comics 共享进度状态 + 预扫总图片数

**Files:**
- Modify: `src-tauri/src/normalize/comic_archive.rs`

引入共享进度状态 `ArchiveShared`（供 command 层轮询线程读取实现卷内逐图实时刷新）。`on_image` 原子递增 done_images；进卷/跳过/失败时更新当前 manga/vol 与 done_images。

- [ ] **Step 1: 写失败测试**

在 `comic_archive.rs` 顶部结构区新增（放在 `ArchiveReport` 之后）：

```rust
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;

/// 归档共享进度状态：并行编码线程原子递增 done_images，command 层轮询线程读取 emit。
pub struct ArchiveShared {
    pub done_images: AtomicUsize,
    pub total_images: AtomicUsize,
    pub total_manga: AtomicUsize,
    pub total_vols: AtomicUsize,
    pub current: Mutex<(String, String)>, // (manga, vol)
}

impl ArchiveShared {
    pub fn new() -> Self {
        ArchiveShared {
            done_images: AtomicUsize::new(0),
            total_images: AtomicUsize::new(0),
            total_manga: AtomicUsize::new(0),
            total_vols: AtomicUsize::new(0),
            current: Mutex::new((String::new(), String::new())),
        }
    }
    /// 生成一份可序列化的进度快照。
    pub fn snapshot(&self) -> ArchiveProgress {
        let (manga, vol) = self.current.lock().unwrap().clone();
        ArchiveProgress {
            manga, vol,
            done_images: self.done_images.load(Ordering::Relaxed),
            total_images: self.total_images.load(Ordering::Relaxed),
            total_manga: self.total_manga.load(Ordering::Relaxed),
            total_vols: self.total_vols.load(Ordering::Relaxed),
        }
    }
}

impl Default for ArchiveShared {
    fn default() -> Self { Self::new() }
}

/// 归档实时进度快照（emit 给前端）。
#[derive(Debug, Clone, serde::Serialize)]
pub struct ArchiveProgress {
    pub manga: String,
    pub vol: String,
    pub done_images: usize,
    pub total_images: usize,
    pub total_manga: usize,
    pub total_vols: usize,
}
```

在 `mod tests` 内新增（用共享状态直接断言累计正确性）：

```rust
    #[test]
    fn shared_progress_totals_and_done() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        touch_img(&root.join("海贼王/第01卷"), "1.png");
        touch_img(&root.join("海贼王/第01卷"), "2.png");
        touch_img(&root.join("海贼王/第02卷"), "1.png");
        touch_img(&root.join("火影/vol1"), "1.png");
        let shared = ArchiveShared::new();
        let reports = archive_comics(root, &shared);
        assert!(reports.iter().all(|r| r.status == "成功"));
        let snap = shared.snapshot();
        assert_eq!(snap.total_images, 4);
        assert_eq!(snap.done_images, 4);
        assert_eq!(snap.total_vols, 3);
        assert_eq!(snap.total_manga, 2);
    }

    #[test]
    fn shared_progress_skipped_counts_images() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        touch_img(&root.join("A/vol1"), "1.png");
        touch_img(&root.join("A/vol1"), "2.png");
        std::fs::write(root.join("A/Vol_01.zip"), b"OLD").unwrap();
        let shared = ArchiveShared::new();
        let reports = archive_comics(root, &shared);
        assert!(reports[0].status.contains("跳过"));
        let snap = shared.snapshot();
        assert_eq!(snap.total_images, 2);
        assert_eq!(snap.done_images, 2);
    }

    #[test]
    fn shared_progress_failed_still_advances() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        std::fs::create_dir_all(root.join("B/vol1")).unwrap();
        std::fs::write(root.join("B/vol1/bad.png"), b"not an image").unwrap();
        touch_img(&root.join("B/vol2"), "1.png");
        let shared = ArchiveShared::new();
        let reports = archive_comics(root, &shared);
        assert!(reports.iter().any(|r| r.status.starts_with("失败")));
        let snap = shared.snapshot();
        assert_eq!(snap.done_images, snap.total_images);
    }
```

- [ ] **Step 2: 运行测试验证失败**

Run: `cd src-tauri && cargo test --lib comic_archive`
Expected: 编译失败——`archive_comics` 签名仍是旧回调、无 ArchiveShared。

- [ ] **Step 3: 改造 archive_comics 用共享状态**

把 `archive_comics` 改为接受 `&ArchiveShared`（不再用 FnMut 回调）：

```rust
pub fn archive_comics(root: &Path, shared: &ArchiveShared) -> Vec<ArchiveReport> {
    let vol_dirs = find_volume_dirs(root);
    let plans = plan_volumes(&vol_dirs);
    let vol_img_counts: Vec<usize> = plans.iter().map(|p| collect_images(&p.dir).len()).collect();
    let total_images: usize = vol_img_counts.iter().sum();
    let mut manga_set = std::collections::BTreeSet::new();
    for p in &plans { manga_set.insert(p.manga.clone()); }
    shared.total_images.store(total_images, Ordering::Relaxed);
    shared.total_vols.store(plans.len(), Ordering::Relaxed);
    shared.total_manga.store(manga_set.len(), Ordering::Relaxed);

    let mut reports = Vec::new();
    for (idx, plan) in plans.iter().enumerate() {
        let vol_name = format!("Vol_{:0width$}", plan.index, width = plan.width);
        let vol_imgs = vol_img_counts[idx];
        // 更新当前 manga/vol（供轮询线程读取）
        *shared.current.lock().unwrap() = (plan.manga.clone(), vol_name.clone());
        let out_zip = plan.manga_dir.join(format!("{vol_name}.zip"));
        if out_zip.exists() {
            shared.done_images.fetch_add(vol_imgs, Ordering::Relaxed); // 跳过卷补加
            reports.push(ArchiveReport {
                manga: plan.manga.clone(), vol: vol_name, status: "跳过(已存在)".into(), pages: 0,
            });
            continue;
        }
        let mut imgs = collect_images(&plan.dir);
        imgs.sort_by(|a, b| {
            let ka = natural_key(a.file_name().and_then(|s| s.to_str()).unwrap_or(""));
            let kb = natural_key(b.file_name().and_then(|s| s.to_str()).unwrap_or(""));
            ka.cmp(&kb)
        });
        let vol_prefix = format!("{:0width$}", plan.index, width = plan.width);
        let base_before = shared.done_images.load(Ordering::Relaxed);
        // on_image 在并行线程内原子递增已处理图片数
        let status_pages = pack_images_to_zip(&imgs, &out_zip,
            move |p| format!("{vol_prefix}_{:03}.jpg", p + 1),
            || { shared.done_images.fetch_add(1, Ordering::Relaxed); });
        match status_pages {
            Ok(n) => {
                // 对齐到实际张数（防并行计数与预扫的细微偏差）
                shared.done_images.store(base_before + n, Ordering::Relaxed);
                reports.push(ArchiveReport {
                    manga: plan.manga.clone(), vol: vol_name, status: "成功".into(), pages: n,
                });
            }
            Err(e) => {
                // 失败卷把该卷图片数补加到 done（进度条不卡）
                shared.done_images.store(base_before + vol_imgs, Ordering::Relaxed);
                reports.push(ArchiveReport {
                    manga: plan.manga.clone(), vol: vol_name, status: format!("失败({e})"), pages: 0,
                });
            }
        }
    }
    // 收尾对齐
    shared.done_images.store(total_images, Ordering::Relaxed);
    reports
}

/// 收集目录内图片路径（is_file + 非 junk + IMG_EXTS）。
fn collect_images(dir: &Path) -> Vec<PathBuf> {
    match std::fs::read_dir(dir) {
        Ok(rd) => rd.filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| {
                if p.is_file() && !junk::is_system_junk_path(p) {
                    if let Some(ext) = p.extension().and_then(|s| s.to_str()) {
                        return IMG_EXTS.contains(&ext.to_lowercase().as_str());
                    }
                }
                false
            }).collect(),
        Err(_) => Vec::new(),
    }
}
```

说明：原 `archive_comics` 内联的图片收集被 `collect_images` 取代；`on_image` 闭包借用 `shared`（`archive_comics` 持 `&ArchiveShared`，闭包 `move` 捕获该引用，`pack_images_to_zip` 的 `on_image: impl Fn()+Sync` 满足——`&AtomicUsize` 是 Sync）。因 `shared` 被 on_image 闭包借用、又在循环内多次用，闭包用 `move` 捕获的是 `&ArchiveShared`（Copy 的引用），无所有权冲突。

- [ ] **Step 4: 运行测试验证通过**

Run: `cd src-tauri && cargo test --lib comic_archive`
Expected: 全 PASS（3 新 + 原有 6）。**原有 6 个测试**（archive_generates_vol_zip_and_reports 等）此前用 `archive_comics(root, |_,_| {})`，需改为 `let shared = ArchiveShared::new(); archive_comics(root, &shared)`（不再传闭包）——一并更新。

- [ ] **Step 5: 提交**

```bash
git add src-tauri/src/normalize/comic_archive.rs
git commit -m "feat(comic_archive): 进度改共享状态 ArchiveShared(原子计数/预扫总图片数/跳过失败补加)

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>"
```

---

### Task 3: command 轮询线程 emit + ipc + 前端三级文案

**Files:**
- Modify: `src-tauri/src/normalize/comic_archive.rs`（command）
- Modify: `src/lib/ipc.ts`
- Modify: `src/views/NormalizeView.ts`

command 层：把 `ArchiveShared` 放进 `Arc`，spawn_blocking 跑归档；同时起一个后台轮询线程每 ~80ms 读 `shared.snapshot()` emit，归档结束后停轮询并 emit 最终一帧。

- [ ] **Step 1: 改造 command**

```rust
#[tauri::command]
pub async fn archive_comics_cmd(
    app: tauri::AppHandle,
    db: tauri::State<'_, crate::db::Db>,
) -> AppResult<Vec<ArchiveReport>> {
    use tauri::Emitter;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};
    let root = crate::settings::get(&db, "comic_root")?
        .ok_or_else(|| crate::error::AppError::Invalid("comic root not set".into()))?;
    let shared = Arc::new(ArchiveShared::new());
    let done_flag = Arc::new(AtomicBool::new(false));

    // 轮询线程：每 80ms emit 一次进度快照，直到 done_flag 置位。
    let poll_shared = shared.clone();
    let poll_done = done_flag.clone();
    let poll_app = app.clone();
    let poller = std::thread::spawn(move || {
        loop {
            let snap = poll_shared.snapshot();
            let _ = poll_app.emit("comic-archive-progress", &snap);
            if poll_done.load(Ordering::Relaxed) {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(80));
        }
    });

    let work_shared = shared.clone();
    let reports = tauri::async_runtime::spawn_blocking(move || {
        archive_comics(Path::new(&root), &work_shared)
    })
    .await
    .map_err(|e| crate::error::AppError::Other(format!("join: {e}")))?;

    // 停轮询 + 最终一帧（100%）
    done_flag.store(true, Ordering::Relaxed);
    let _ = poller.join();
    let _ = app.emit("comic-archive-progress", &shared.snapshot());
    Ok(reports)
}
```

> 轮询线程在归档跑完置 `done_flag` 后再循环一次 emit 最终快照并退出，command 末尾再补 emit 一帧确保前端拿到 100%。80ms 间隔避免高频洪水。

- [ ] **Step 2: 更新 ipc.ts**

在 `src/lib/ipc.ts` 接口区新增：

```ts
export interface ArchiveProgress { manga: string; vol: string; done_images: number; total_images: number; total_manga: number; total_vols: number; }
```

（`archiveComics` 调用不变。）

- [ ] **Step 3: 前端渲染三级进度**

在 `NormalizeView.ts` 的 `#comic-run` onclick 内，把 listen 段替换为：

```ts
      unlistenC = await listen<{ manga: string; vol: string; done_images: number; total_images: number }>("comic-archive-progress", (e) => {
        const { manga, vol, done_images, total_images } = e.payload;
        const pct = total_images ? Math.round((done_images / total_images) * 100) : 0;
        bar.style.width = pct + "%";
        text.textContent = manga
          ? `正在：${manga} ${vol}｜图片 ${done_images}/${total_images}`
          : `准备中…`;
      });
```

（其余逻辑不变。）

- [ ] **Step 4: 全量测试 + 类型检查**

Run: `cd src-tauri && cargo test --lib 2>&1 | tail -3`
Expected: 全 PASS。

Run: `cd .. && npx tsc --noEmit`
Expected: 无输出。

- [ ] **Step 5: 提交**

```bash
git add src-tauri/src/normalize/comic_archive.rs src/lib/ipc.ts src/views/NormalizeView.ts
git commit -m "feat(archive): command 后台轮询线程实时 emit 卷内逐图进度，前端显示漫画/卷/图片

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>"
```

---

## 自查

- **规格覆盖**：on_image 回调（Task 1）、预扫总图片数 + 共享状态 ArchiveShared + 跳过/失败补加 + 末值对齐（Task 2）、command 后台轮询线程实时 emit + 最终帧（Task 3 Step 1）、ipc 新接口（Task 3 Step 2）、前端进度条按图片总数 + 三级文案（Task 3 Step 3）——规格各条均有对应任务。用户明确要"卷内逐图实时刷新"：由 command 轮询线程读原子计数器实现（Task 3）。
- **占位符扫描**：无 TBD/TODO；每步给出完整代码。
- **类型/命名一致性**：`ArchiveProgress` 字段在 Task 2 定义、`ArchiveShared::snapshot` 产出、Task 3 command emit、ipc 接口、前端 listen 逐一一致；`ArchiveShared`（done_images/total_*/current）在 Task 2 定义、Task 3 command 用；`pack_images_to_zip` 新签名（+on_image）在 Task 1 定义、Task 2 调用一致；`collect_images` 在 Task 2 定义并复用。
- **线程安全**：`on_image` 借 `&AtomicUsize`（Sync），满足 `pack_images_to_zip` 的 `Fn()+Sync` 约束；轮询线程与工作线程通过 `Arc<ArchiveShared>` 共享，读写皆原子/Mutex，无数据竞争。
