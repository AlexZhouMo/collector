# 代码重构 · 删除冗余与废弃文件/代码 — 设计

## 背景与目标

项目 `collector` 经多轮演进（早期 Java 字幕工具 → Tauri+Vite；游戏模块 category 列反复重构；漫画封面抓取源 AniList/Bangumi/wikicover 先加后删；封面前缀统一 cover_/tmdb_），磁盘与代码里积累了残留。

目标：删除已确认冗余/废弃的文件、静态资源、死代码片段，不改变任何现有功能。所有删除项均经两个 Explore 代理排查 + 逐项 grep/cargo 校验确认无引用。

## 删除清单（全部已验证）

### A. 磁盘垃圾（未被 git 跟踪，删除零风险）
- `.classpath`、`.project`、`.settings/`（含 `org.eclipse.jdt.core.prefs`）— Eclipse Java 项目残留（项目名 "subtitle"、JavaSE-1.8），与 TS/Rust 技术栈无关。`.gitignore` 已忽略。
- `scripts/__pycache__/` — Python 字节码缓存。

### B. 前端死代码（grep 全仓零引用）
- `src/components/PosterGrid.ts` — 孤儿模块，无任何 import 者。
- `src/lib/icons.ts` 的 `export ... iconText` — 未使用导出（删函数定义，保留其余导出如 `icon`）。
- `src/lib/videoTree.ts` 的 `export ... collectFolderPaths` — 未使用导出（保留 `buildVideoTree`/`findNode`）。
- `src/assets/tauri.svg`、`src/assets/typescript.svg`、`src/assets/vite.svg` — Vite/Tauri 脚手架遗留 logo，代码/HTML/CSS 零引用。

### C. 后端死代码（cargo 报 dead_code，仅测试自引用）
- `src-tauri/src/library/paths.rs`：删 `video_to_absolute`、`video_to_relative`、`strip_category` 三个函数 + 各自 `#[test]`（`video_rel_abs_roundtrip`、`video_empty_root_passthrough`、`strip_category_works`）。均为路径存储方案变更/category 列重构残留。
- `src-tauri/src/normalize/subtitle_check.rs`：删 `check` 函数 + 其测试（`flags_suspicious_punct`）。**保留** `check_timeline_cross` 和 `Issue`（仍被 subtitle.rs/mod.rs 使用）。

### D. 前后端联动死命令链（无 UI 调用入口）
- `src-tauri/src/lib.rs`：删 `fn init_from_demo`（约 77-89 行）及 `invoke_handler` 里的注册行（约 680 行）。
- `src/lib/ipc.ts`：删 `initFromDemo` 包装（约 30 行）。

### E. 临时产物
- `poster-candidates/` — 会话中生成的海报候选/对照表，非项目代码。整目录删除。

## 明确保留（不动）
- `docs/subtitles/` 字幕样本、`docs/superpowers/` plans+specs 计划归档。
- `scripts/poster_regression.py` + `poster_baseline.json` + `test-moveTreeFilter.ts`（**用户明确保留**）+ `relocate-libmpv.sh`（打包必需）。
- `src/assets/fonts/NotoSansCJKsc-Regular.woff2` + `README.md`、`public/libass/` 三件套（含 `-legacy.js`，SubtitleRenderer 实际引用）。
- `@tauri-apps/plugin-opener`（前后端仅注册未调用，但删除属破坏性、收益小，保留待后续需要时再定）。
- `dist/`（构建产物，已被 .gitignore 忽略，不入库，无需处理）。

## 执行顺序与验证
1. 删 C/D 的代码片段（后端函数+命令、前端导出+包装）。
2. 删 B 的孤儿文件/资源、E 临时产物、A 磁盘垃圾。
3. 前端验证：`npx tsc --noEmit` 通过（无因删除导致的类型/引用错误）。
4. 后端验证：`cd src-tauri && cargo build` 无 dead_code 警告（原 4 项消除）、`cargo test --lib` 全绿。
5. `git status` 复核改动范围符合清单。

## 不做（YAGNI）
- 不删任何仍被引用的资源/依赖。
- 不动 docs 归档与字幕样本、不动测试脚本、不动 plugin-opener。
- 不做与"删冗余"无关的重构（不重命名、不重组模块、不改逻辑）。

## 风险
- 全部删除项已 grep/cargo 双重验证无引用；删除后靠 tsc + cargo test 兜底。
- 删代码片段时注意只删目标函数/导出，不误伤同文件其它成员（尤其 subtitle_check.rs 保留 check_timeline_cross）。
