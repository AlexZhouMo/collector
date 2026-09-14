# 删除冗余/废弃 实现计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 删除已验证的冗余/废弃文件、静态资源、死代码片段，不改变任何现有功能。

**Architecture:** 纯删除操作。分 5 组：前端死代码、后端死代码、前后端联动死命令、废弃文件/资源、磁盘垃圾。每组删完即验证（tsc / cargo）。

**Tech Stack:** TS(tsc)、Rust(cargo)、git、文件系统。

---

## 前置：确认工作树基线
- [ ] **Step 1: 记录基线**

Run: `cd /Users/zhoumo/Documents/Claude/collector && git status --short && cd src-tauri && cargo build 2>&1 | grep -c "never used"`
Expected: 工作树仅 `?? poster-candidates/`；cargo dead_code 警告数为 4。

## Task 1: 前端死代码删除

**Files:**
- Delete: `src/components/PosterGrid.ts`
- Delete: `src/assets/tauri.svg`, `src/assets/typescript.svg`, `src/assets/vite.svg`
- Modify: `src/lib/icons.ts`（删 `iconText`）
- Modify: `src/lib/videoTree.ts`（删 `collectFolderPaths`）

- [ ] **Step 1: 删孤儿文件与脚手架 svg**

```bash
cd /Users/zhoumo/Documents/Claude/collector
rm -f src/components/PosterGrid.ts src/assets/tauri.svg src/assets/typescript.svg src/assets/vite.svg
```

- [ ] **Step 2: 删 icons.ts 的 iconText 导出**

`src/lib/icons.ts:37` 起的整个 `export function iconText(name: IconName, text: string, size = 16): string { ... }` 函数（用 Edit 精确匹配函数体，含其闭合 `}`）。删除后确认 `icon`、`IconName` 等其它导出保留。

- [ ] **Step 3: 删 videoTree.ts 的 collectFolderPaths 导出**

`src/lib/videoTree.ts:52` 起的整个 `export function collectFolderPaths(root: TreeNode): string[] { ... }` 函数（Edit 精确匹配含闭合）。保留 `buildVideoTree`、`findNode`、`TreeNode` 类型。

- [ ] **Step 4: 验证前端**

Run: `cd /Users/zhoumo/Documents/Claude/collector && npx tsc --noEmit`
Expected: 无错误输出（退出码 0）。

- [ ] **Step 5: commit**

```bash
git add -A && git commit -m "refactor: 删前端死代码(PosterGrid/iconText/collectFolderPaths/脚手架svg)

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>"
```

## Task 2: 后端死代码删除

**Files:**
- Modify: `src-tauri/src/library/paths.rs`
- Modify: `src-tauri/src/normalize/subtitle_check.rs`

- [ ] **Step 1: paths.rs 删三个死函数**

用 Edit 删除以下三段（连同各自的 doc 注释）：
- 第 14-28 行：`/// 绝对视频路径...` + `pub fn video_to_relative(...)` + `/// 相对视频路径...` + `pub fn video_to_absolute(...)`（两个函数连注释一起删）。
- 第 76-81 行：`/// 去掉 category_path 首段...` + `pub fn strip_category(...)`。
保留 `video_root_key`、`video_abs_path`、`appdata_to_relative/absolute`、`subtitle_rel_path/abs_path`。

- [ ] **Step 2: paths.rs 删对应测试**

删测试 mod 内三个 `#[test]`：`video_rel_abs_roundtrip`（86-92）、`video_empty_root_passthrough`（93-99）、`strip_category_works`（108-112）。保留 `appdata_rel_abs`、`video_abs_path_joins`、`subtitle_paths`。

- [ ] **Step 3: subtitle_check.rs 删 check 函数 + 修 import**

- 删第 4-22 行：`/// 对标准化后文本做质检...` + `pub fn check(content: &str) -> Vec<Issue> { ... }`（整函数含 doc）。
- 删第 53-62 行的 `#[cfg(test)] mod tests { ... }`（只含 `flags_suspicious_punct`，它调用了被删的 check）。
- 改第 1 行 import：`use crate::normalize::subtitle::{parse_dialogues, parse_time_cs, Dialogue, SEPARATOR};` → `use crate::normalize::subtitle::{parse_time_cs, Dialogue};`（`parse_dialogues`/`SEPARATOR` 仅被 check 用，删后变未用）。
- 保留第 2 行 `pub use ...Issue;`、`check_timeline_cross`（24-51）、`cross_tests`（64-92）。

- [ ] **Step 4: 验证后端 dead_code 清零**

Run: `cd /Users/zhoumo/Documents/Claude/collector/src-tauri && cargo build 2>&1 | grep -c "never used"`
Expected: `0`（原 4 项消除，且未引入新的未用 import 警告）。

- [ ] **Step 5: 后端测试全绿**

Run: `cd /Users/zhoumo/Documents/Claude/collector/src-tauri && cargo test --lib 2>&1 | tail -5`
Expected: `test result: ok.`，failed 0。

- [ ] **Step 6: commit**

```bash
cd /Users/zhoumo/Documents/Claude/collector
git add -A && git commit -m "refactor: 删后端死代码(video_to_*/strip_category/subtitle check)

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>"
```

## Task 3: 前后端联动死命令 init_from_demo

**Files:**
- Modify: `src-tauri/src/lib.rs`（删 fn + 注册行）
- Modify: `src/lib/ipc.ts`（删包装）

- [ ] **Step 1: 删 lib.rs 的 init_from_demo 定义**

Edit 删除 `lib.rs` 中这一整块（含 doc 注释）：
```
/// 从 demo/subtitles 初始化导入视频库：扫电影/动漫/剧集三目录的 .ass 为条目，
/// 清库（含 id 序列重置）+ 排序顺序插入。demo_root 由前端传入（demo/subtitles 绝对路径）。
#[tauri::command(rename_all = "camelCase")]
fn init_from_demo(db: tauri::State<Db>, demo_root: String) -> AppResult<usize> {
    ... (整个函数体到闭合 } )
}
```

- [ ] **Step 2: 删 invoke_handler 注册行**

Edit 删除 `lib.rs` 约 680 行的 `            init_from_demo,` 一行。

- [ ] **Step 3: 删 ipc.ts 包装**

Edit 删除 `src/lib/ipc.ts:30` 的 `initFromDemo: (demoRoot: string) => invoke<number>("init_from_demo", { demoRoot }),` 一行。

- [ ] **Step 4: 双端验证**

Run: `cd /Users/zhoumo/Documents/Claude/collector && npx tsc --noEmit && cd src-tauri && cargo build 2>&1 | tail -3`
Expected: tsc 无错误；cargo build 成功，无 init_from_demo 相关未用警告。

- [ ] **Step 5: commit**

```bash
cd /Users/zhoumo/Documents/Claude/collector
git add -A && git commit -m "refactor: 删无UI入口的 init_from_demo 死命令链(前后端)

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>"
```

## Task 4: 废弃文件/临时产物 + 磁盘垃圾

**Files:** Delete only

- [ ] **Step 1: 删临时产物与磁盘垃圾**

```bash
cd /Users/zhoumo/Documents/Claude/collector
rm -rf poster-candidates
rm -f .classpath .project
rm -rf .settings
rm -rf scripts/__pycache__
```
（poster-candidates 未入库、Eclipse 文件未入库，均不影响 git；scripts/__pycache__ 同理。）

- [ ] **Step 2: 确认删除**

Run: `cd /Users/zhoumo/Documents/Claude/collector && ls -la | grep -E "classpath|project|settings|poster-candidates" ; ls scripts/`
Expected: 无 .classpath/.project/.settings/poster-candidates；scripts/ 仅剩 poster_regression.py、poster_baseline.json、relocate-libmpv.sh、test-moveTreeFilter.ts（测试脚本按用户要求保留）。

- [ ] **Step 3: git 复核**（这些多为未跟踪文件，可能无需 commit）

Run: `cd /Users/zhoumo/Documents/Claude/collector && git status --short`
Expected: 工作树干净或仅无关变更（Eclipse/poster-candidates 本就未跟踪，删除不产生 git 变更）。

## 最终验证
- [ ] **Step 1: 全量验证**

Run: `cd /Users/zhoumo/Documents/Claude/collector && npx tsc --noEmit && cd src-tauri && cargo build 2>&1 | grep -c "never used" && cargo test --lib 2>&1 | grep "test result"`
Expected: tsc 无错；dead_code 计数 0；cargo test result ok。

## 明确不动
- 保留：docs/subtitles 字幕样本、docs/superpowers plans+specs、scripts 全部脚本（含测试脚本，用户要求）、relocate-libmpv.sh、字体 woff2+README、public/libass 三件套(含 legacy)、@tauri-apps/plugin-opener、dist/(已忽略)。
- 不重命名、不重组模块、不改任何业务逻辑。

## Self-Review
- Spec 覆盖：A磁盘垃圾(Task4)✓ B前端(Task1)✓ C后端(Task2)✓ D命令链(Task3)✓ E临时产物+测试脚本保留(Task4)✓。
- 无占位符：删除点均给出文件+行号+匹配内容；subtitle_check import 调整已含。
- 一致性：subtitle_check 删 check 后同步改 import（parse_dialogues/SEPARATOR 移除），避免新 unused 警告。
- 验证闭环：每 Task 删后即 tsc/cargo，最终全量复验。
