# 系统冗余文件统一过滤 实现计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 新增一个集中的"系统冗余文件"判定模块，并在所有目录遍历点主动排除 `.DS_Store`、`Thumbs.db` 等系统自动生成文件。

**Architecture:** 纯函数模块 `util::junk`（无 IO、无项目内依赖），导出 `is_system_junk(name)` 与 `is_system_junk_path(path)`。各遍历点 `use crate::util::junk` 后在筛选处调用。对现有真实数据无可见行为变化——价值是把"靠扩展名白名单顺带排除"变为显式、集中、有测试锚定的规则。

**Tech Stack:** Rust（仅 std），cargo test。

---

### Task 1: 新增 `util::junk` 判定模块

**Files:**
- Create: `src-tauri/src/util.rs`
- Modify: `src-tauri/src/lib.rs`（在 `mod settings;` 后新增 `mod util;`）

- [ ] **Step 1: 写失败测试**

创建 `src-tauri/src/util.rs`，内容如下（先只写模块骨架 + 测试，函数体用 `todo!()` 让测试编译通过但运行失败）：

```rust
//! 通用工具。

pub mod junk {
    use std::path::Path;

    /// 系统/工具自动生成的冗余文件名清单（比较时统一转小写）。
    const JUNK_NAMES: &[&str] = &[
        ".ds_store",       // macOS Finder
        "thumbs.db",       // Windows 缩略图缓存
        "desktop.ini",     // Windows 文件夹配置
        ".spotlight-v100", // macOS 索引
        ".trashes",        // macOS 废纸篓
        ".fseventsd",      // macOS 文件系统事件
    ];

    /// 判断一个文件/目录名是否为系统冗余文件：
    /// 命中 JUNK_NAMES（大小写不敏感），或以 `._` 开头（macOS AppleDouble 派生文件）。
    pub fn is_system_junk(name: &str) -> bool {
        todo!()
    }

    /// 便捷版：取路径末段文件名判定；无末段（如根路径）返回 false。
    pub fn is_system_junk_path(path: &Path) -> bool {
        todo!()
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        use std::path::Path;

        #[test]
        fn matches_known_junk_names() {
            assert!(is_system_junk(".DS_Store"));
            assert!(is_system_junk("Thumbs.db"));
            assert!(is_system_junk("Desktop.ini"));
            assert!(is_system_junk(".Spotlight-V100"));
            assert!(is_system_junk(".Trashes"));
            assert!(is_system_junk(".fseventsd"));
        }

        #[test]
        fn case_insensitive() {
            assert!(is_system_junk("thumbs.DB"));
            assert!(is_system_junk(".ds_store"));
            assert!(is_system_junk("THUMBS.DB"));
            assert!(is_system_junk("DESKTOP.INI"));
        }

        #[test]
        fn apple_double_prefix() {
            assert!(is_system_junk("._星战.mkv"));
            assert!(is_system_junk("._foo"));
            assert!(is_system_junk("._"));
        }

        #[test]
        fn normal_files_not_flagged() {
            assert!(!is_system_junk("01.jpg"));
            assert!(!is_system_junk("星战.ass"));
            assert!(!is_system_junk("poster.jpg"));
            assert!(!is_system_junk("game.json"));
            assert!(!is_system_junk("info.txt"));
            assert!(!is_system_junk(".gitignore")); // 单个点前缀不误判
        }

        #[test]
        fn empty_name_not_junk() {
            assert!(!is_system_junk(""));
        }

        #[test]
        fn path_helper() {
            assert!(is_system_junk_path(Path::new("/a/b/.DS_Store")));
            assert!(is_system_junk_path(Path::new("/a/b/._x.mkv")));
            assert!(!is_system_junk_path(Path::new("/a/b/星战.mkv")));
            assert!(!is_system_junk_path(Path::new("/"))); // 无末段文件名
        }
    }
}
```

在 `src-tauri/src/lib.rs` 的 `mod settings;` 之后新增一行：

```rust
mod util;
```

- [ ] **Step 2: 运行测试验证失败**

Run: `cd src-tauri && cargo test --lib junk`
Expected: 编译通过，测试 panic（`not yet implemented` / `todo!`）。

- [ ] **Step 3: 实现函数体**

把 `is_system_junk` 与 `is_system_junk_path` 的 `todo!()` 替换为实现：

```rust
    pub fn is_system_junk(name: &str) -> bool {
        if name.is_empty() {
            return false;
        }
        if name.starts_with("._") {
            return true;
        }
        let lower = name.to_lowercase();
        JUNK_NAMES.contains(&lower.as_str())
    }

    pub fn is_system_junk_path(path: &Path) -> bool {
        path.file_name()
            .and_then(|s| s.to_str())
            .map(is_system_junk)
            .unwrap_or(false)
    }
```

- [ ] **Step 4: 运行测试验证通过**

Run: `cd src-tauri && cargo test --lib junk`
Expected: 全部 PASS（6 个测试）。

- [ ] **Step 5: 提交**

```bash
git add src-tauri/src/util.rs src-tauri/src/lib.rs
git commit -m "feat(util): 新增系统冗余文件判定 util::junk(.DS_Store/Thumbs.db 等)

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>"
```

---

### Task 2: 视频/字幕/漫画扫描接入过滤

**Files:**
- Modify: `src-tauri/src/library/scanner.rs`（`scan_videos` / `scan_videos_subs` / `scan_comics` / `scan_games` 及其 `use`）

- [ ] **Step 1: 写失败测试**

在 `scanner.rs` 的 `mod tests` 内（`scans_nested_categories_and_sidecars` 测试所在模块）新增：

```rust
    #[test]
    fn scan_videos_skips_system_junk() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let dir = root.join("科幻");
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("星战.mkv"), b"x").unwrap();
        fs::write(dir.join(".DS_Store"), b"junk").unwrap();
        fs::write(dir.join("Thumbs.db"), b"junk").unwrap();
        let items = scan_videos(root, "电影");
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].title, "星战");
    }
```

在 `mod comic_scan_tests` 内新增：

```rust
    #[test]
    fn scan_comics_skips_system_junk() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("热血");
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("第01卷.zip"), b"PK").unwrap();
        fs::write(dir.join(".DS_Store"), b"junk").unwrap();
        let items = scan_comics(tmp.path());
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].title, "第01卷");
    }
```

> 说明：因 `.mkv`/`.zip` 白名单本就排除这些文件，未接入前这两个测试**也会通过**——它们是回归锚点，确保接入后行为不变、且将来白名单放宽时仍安全。为验证接入点被真正调用，Step 2 只确认测试编译通过。

- [ ] **Step 2: 运行测试（确认锚点存在）**

Run: `cd src-tauri && cargo test --lib scanner`
Expected: 全部 PASS（含新增两个）。

- [ ] **Step 3: 接入过滤**

在 `scanner.rs` 顶部 `use walkdir::WalkDir;` 后新增：

```rust
use crate::util::junk;
```

在 `scan_videos` 的 `for entry in ...` 循环体，`let p = entry.path();` 之后、`if p.extension()...` 之前插入：

```rust
        if junk::is_system_junk_path(p) {
            continue;
        }
```

对 `scan_videos_subs` 做**完全相同**的插入（同样在 `let p = entry.path();` 之后、`if p.extension()...` 之前）：

```rust
        if junk::is_system_junk_path(p) {
            continue;
        }
```

对 `scan_comics` 做**完全相同**的插入（同样在 `let p = entry.path();` 之后、`if p.extension()...` 之前）：

```rust
        if junk::is_system_junk_path(p) {
            continue;
        }
```

在 `scan_games` 的 `for e in entries.filter_map(|e| e.ok())` 循环体，`if !dir.is_dir() { continue; }` 之后插入（防御式，目录一般不叫这些名字，但保持一致）：

```rust
        if junk::is_system_junk_path(&dir) {
            continue;
        }
```

- [ ] **Step 4: 运行测试验证通过**

Run: `cd src-tauri && cargo test --lib scanner`
Expected: 全部 PASS。

- [ ] **Step 5: 提交**

```bash
git add src-tauri/src/library/scanner.rs
git commit -m "feat(scanner): 扫描视频/字幕/漫画/游戏时排除系统冗余文件

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>"
```

---

### Task 3: 漫画打包与字幕校准接入过滤

**Files:**
- Modify: `src-tauri/src/normalize/comic_pack.rs`（`pack_comic_dir` 的 filter 及 `use`）
- Modify: `src-tauri/src/normalize/mod.rs`（`run_subtitle_normalize` 的 filter 及 `use`）

- [ ] **Step 1: 写失败测试**

在 `comic_pack.rs` 的 `mod tests` 内、已有 `packs_images_into_renamed_jpg_zip` 测试中，源目录写入文件的循环处，把 `.DS_Store` 也加入并断言被忽略。将该测试的文件名数组与断言改为：

```rust
    #[test]
    fn packs_images_into_renamed_jpg_zip() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("src");
        std::fs::create_dir_all(&dir).unwrap();
        for name in ["b.png","a.png","Thumbs.db",".DS_Store"] {
            if name.ends_with(".png") {
                let img = RgbImage::from_pixel(4, 4, Rgb([10, 20, 30]));
                img.save(dir.join(name)).unwrap();
            } else {
                std::fs::write(dir.join(name), b"junk").unwrap();
            }
        }
        let out = tmp.path().join("out.zip");
        let n = pack_comic_dir(&dir, "海贼王01", &out).unwrap();
        assert_eq!(n, 2); // 仅两张图，Thumbs.db 与 .DS_Store 被忽略
        let mut ar = zip::ZipArchive::new(File::open(&out).unwrap()).unwrap();
        let names: Vec<String> = (0..ar.len()).map(|i| ar.by_index(i).unwrap().name().to_string()).collect();
        assert!(names.contains(&"海贼王01_001.jpg".to_string()));
        assert!(names.contains(&"海贼王01_002.jpg".to_string()));
    }
```

在 `normalize/mod.rs` 的 `mod tests` 内新增：

```rust
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
```

- [ ] **Step 2: 运行测试（确认锚点）**

Run: `cd src-tauri && cargo test --lib comic_pack && cargo test --lib normalize`
Expected: 全部 PASS（`.ass`/图片白名单已顺带排除，此步确认锚点存在）。

- [ ] **Step 3: 接入过滤**

在 `comic_pack.rs` 顶部 `use std::path::Path;` 后新增：

```rust
use crate::util::junk;
```

修改 `pack_comic_dir` 的 `.filter(...)` 闭包，在图片白名单判断上追加排除冗余（把整段 `l.ends_with(...)` 的返回值与 `!is_system_junk_path` 相与）：

```rust
        .filter(|p| {
            if junk::is_system_junk_path(p) {
                return false;
            }
            let l = p.to_string_lossy().to_lowercase();
            l.ends_with(".jpg") || l.ends_with(".jpeg") || l.ends_with(".png") || l.ends_with(".webp") || l.ends_with(".bmp")
        })
```

在 `normalize/mod.rs` 顶部 `use walkdir::WalkDir;` 后新增：

```rust
use crate::util::junk;
```

修改 `run_subtitle_normalize` 内收集 `.ass` 的 `.filter(...)`，追加排除冗余：

```rust
    let files: Vec<_> = WalkDir::new(in_dir)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| !junk::is_system_junk_path(e.path()))
        .filter(|e| e.path().extension().and_then(|s| s.to_str()) == Some("ass"))
        .map(|e| e.path().to_path_buf())
        .collect();
```

- [ ] **Step 4: 运行测试验证通过**

Run: `cd src-tauri && cargo test --lib comic_pack && cargo test --lib normalize`
Expected: 全部 PASS。

- [ ] **Step 5: 提交**

```bash
git add src-tauri/src/normalize/comic_pack.rs src-tauri/src/normalize/mod.rs
git commit -m "feat(normalize): 漫画打包与字幕校准遍历时排除系统冗余文件

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>"
```

---

### Task 4: zip 内页面列举防御 + 全量测试

**Files:**
- Modify: `src-tauri/src/comic/reader.rs`（`list_pages` 及 `use`）

- [ ] **Step 1: 写失败测试**

在 `reader.rs` 的 `mod tests` 内，修改 `make_zip` 让它同时写入一个 `__MACOSX/._001.jpg` 混入项，并新增断言只列出真实图片。改为：

```rust
    fn make_zip(path: &Path) {
        let f = File::create(path).unwrap();
        let mut w = zip::ZipWriter::new(f);
        let opt = SimpleFileOptions::default();
        for name in ["003.jpg", "001.jpg", "002.jpg", "readme.txt", "__MACOSX/._001.jpg", ".DS_Store"] {
            w.start_file(name, opt).unwrap();
            w.write_all(name.as_bytes()).unwrap();
        }
        w.finish().unwrap();
    }
```

`lists_sorted_image_pages_only` 断言保持 `vec!["001.jpg", "002.jpg", "003.jpg"]` 不变——`._001.jpg` 与 `.DS_Store` 必须被排除。

- [ ] **Step 2: 运行测试验证失败**

Run: `cd src-tauri && cargo test --lib reader`
Expected: `lists_sorted_image_pages_only` FAIL——`__MACOSX/._001.jpg` 以 `.jpg` 结尾会被当作图片列入，结果多一项。

- [ ] **Step 3: 接入过滤**

在 `reader.rs` 顶部 `use zip::ZipArchive;` 后新增：

```rust
use crate::util::junk;
```

修改 `list_pages` 的 `.filter(...)`，先按末段文件名排除冗余（zip 条目名含 `/`，取末段判定）：

```rust
        .filter(|n| {
            let base = n.rsplit('/').next().unwrap_or(n);
            if junk::is_system_junk(base) {
                return false;
            }
            let l = n.to_lowercase();
            l.ends_with(".jpg") || l.ends_with(".jpeg") || l.ends_with(".png")
        })
```

- [ ] **Step 4: 运行测试验证通过**

Run: `cd src-tauri && cargo test --lib reader`
Expected: PASS。

- [ ] **Step 5: 全量测试 + 提交**

Run: `cd src-tauri && cargo test --lib`
Expected: 全部 PASS。

```bash
git add src-tauri/src/comic/reader.rs
git commit -m "feat(reader): zip 内页面列举排除 .DS_Store/AppleDouble 等冗余项

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>"
```

---

## 自查

- **规格覆盖**：util 模块（Task 1）、5 个磁盘遍历接入点（scan_videos/subs/comics/games、pack_comic_dir、run_subtitle_normalize，Task 2+3）、zip 内页面防御（Task 4）——规格全部接入点均有对应任务。
- **占位符扫描**：无 TBD/TODO；每个含代码的步骤都给出完整代码。`todo!()` 是 TDD 的红灯占位，Step 3 即替换。
- **类型一致性**：`is_system_junk(name: &str) -> bool`、`is_system_junk_path(path: &Path) -> bool` 在 Task 1 定义，Task 2/3/4 的调用签名与之一致；`is_system_junk_path` 收 `&Path`，`is_system_junk` 收 `&str`（reader 里对 zip 条目末段字符串用 `is_system_junk`）。
