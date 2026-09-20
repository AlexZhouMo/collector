# 代码清理重构第二轮 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 清理代码冗余（DIAG 残留、空模块、死图标、失效注释、过度 export）并对 MoveDialog 两个 ~80% 重复的函数去重。行为不变。

**Architecture:** A 类零风险清理按前后端分两个任务；MoveDialog 去重单独一个任务（提取公共弹窗、参数化差异，需真机回归）。

**Tech Stack:** Rust (Tauri 2)、TypeScript (Vite)。

**验证：** `cd src-tauri && cargo test && cargo build`；`cd .. && npx tsc --noEmit && npm run build`；MoveDialog 改动后 `npm run tauri:dev` 真机验证移动。

**范围外**：不碰工作树里 440 项字幕目录变动（数据）；不做 scanner 重命名/PlayerView 拆分；保留 comic/game category 预留列。**提交时只 `git add` 明确的代码文件，不 `git add -A`（避免裹挟字幕变动）。**

---

## Task 1: 后端零风险清理（DIAG + 空模块 + 失效注释）

**Files:**
- Modify: `src-tauri/src/player/mod.rs`（删 DIAG）、`src-tauri/src/library/mod.rs`（删 mod 声明）、`src-tauri/src/player/transcode.rs`（注释+改名）、`src-tauri/src/poster/image_proc.rs`（注释）
- Delete: `src-tauri/src/library/cover.rs`

- [ ] **Step 1: 删 DIAG 残留**

`src-tauri/src/player/mod.rs` 第 190-191 行删除：
```rust
    eprintln!("[DIAG worker-end] saw_end={saw_end} part字节={} success={success}",
        part.metadata().map(|m| m.len()).unwrap_or(0));
```
（`success` 变量若删 eprintln 后变未使用，检查其后续是否还用到——它用于 `if success && ...` 分支，保留变量本身。只删这两行 eprintln。）

- [ ] **Step 2: 删空模块 cover.rs**

Run: `git rm src-tauri/src/library/cover.rs`
然后删 `src-tauri/src/library/mod.rs` 里的 `pub mod cover;` 声明行。

- [ ] **Step 3: transcode.rs 失效注释 + fmp4_args 改名**

`src-tauri/src/player/transcode.rs`：
- 模块头注释（约 1-6 行）：改掉 "fragmented MP4 / empty_moov / 用 empty_moov 替代 faststart" 等描述，改为反映现状："mkv 用 ffmpeg `-c copy` 无损转封装成普通 MP4（moov 留尾，`<video>` 经 HTTP Range 探尾播），音频 AC-3 直接 copy（macOS 原生支持）"。
- `fmp4_args` 函数改名为 `remux_args`（函数定义 + 调用处 + 测试名 `transcode_args_are_pure_copy_no_reencode` 里的调用；grep `fmp4_args` 找全部引用改）。
- 函数上的注释同步反映"普通 MP4 remux、-c copy、moov 留尾"。

`src-tauri/src/player/mod.rs` 第 1 行模块头注释：若提到 fragmented/faststart，改为反映现状（普通 mp4、moov 留尾）。

- [ ] **Step 4: image_proc.rs 注释**

`src-tauri/src/poster/image_proc.rs` 约 54 行：注释 "命名 tmdb_<hash>.jpg" 改为 "命名 `<prefix><hash>.jpg`（prefix 由调用方传，tmdb_/cover_ 两用）"。

- [ ] **Step 5: 编译 + 测试**

Run: `cd src-tauri && cargo build 2>&1 | grep -iE "error|warning" || echo clean; cargo test 2>&1 | grep "test result:" | head -1`
Expected: clean（无未使用 mod/变量 warning）+ 测试全过（含改名后的 transcode 测试）。

- [ ] **Step 6: 提交**
```bash
cd /Users/zhoumo/Documents/Claude/collector
git add src-tauri/src/player/mod.rs src-tauri/src/library/mod.rs src-tauri/src/library/cover.rs src-tauri/src/player/transcode.rs src-tauri/src/poster/image_proc.rs
git commit -m "chore(cleanup): 删 DIAG 残留/空模块 cover.rs，订正失效注释，fmp4_args→remux_args

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>"
```
（cover.rs 已 git rm，会在暂存区标删除。）

---

## Task 2: 前端零风险清理（死图标 + 过度 export + README）

**Files:**
- Modify: `src/lib/icons.ts`、`src/lib/normalizeStore.ts`、`src/components/CoverCropper.ts`、`src/lib/ipc.ts`、`src/assets/fonts/README.md`

- [ ] **Step 1: 删死图标 grid**

`src/lib/icons.ts`：删 `IconName` 联合类型里的 `"grid"`、删 `PATHS` 里的 `grid: \`...\`` 定义。（grep 确认全 src 无 `icon("grid")`。）

- [ ] **Step 2: 收敛过度 export**

逐个去掉无外部 import 的 `export`（保留标识符本身，仅去 export 关键字；若某类型作嵌套类型仍需存在就保留定义、只去 export）：
- `src/lib/normalizeStore.ts`：`renderPosterTable`（函数，仅同文件 startPoster 用）→ 去 export；`TaskStatus`、`TaskState` 类型 → 去 export。
- `src/components/CoverCropper.ts`：`screenRectToImageRect`、`ScreenRect` → 去 export；顺带删/改其"便于单测"的失效注释（项目无测试框架）。
- `src/lib/ipc.ts`：`SubIssue`、`FetchReport`、`DbResetResult`、`MissingCover`、`CleanCoversResult` → 去 export（它们在本文件作 invoke 返回/嵌套类型用，无外部 import）。
- **以 tsc 通过为准**：去 export 后若报某标识符问题，按 tsc 提示调整（该保留 export 的保留）。

- [ ] **Step 3: 字体 README 订正**

`src/assets/fonts/README.md`：把 "JASSUB / defaultFont" 相关描述改为 "SubtitlesOctopus / fallbackFont"（与 SubtitleRenderer.ts 实际一致）。字体文件勿删。

- [ ] **Step 4: 类型检查 + 构建**

Run: `cd /Users/zhoumo/Documents/Claude/collector && npx tsc --noEmit && echo tsc-ok; npm run build 2>&1 | tail -1`
Expected: tsc-ok（无未使用/无报错）+ 构建成功。

- [ ] **Step 5: 提交**
```bash
git add src/lib/icons.ts src/lib/normalizeStore.ts src/components/CoverCropper.ts src/lib/ipc.ts src/assets/fonts/README.md
git commit -m "chore(cleanup): 删死图标 grid、收敛过度 export、订正字体 README

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>"
```

---

## Task 3: MoveDialog 去重

**Files:**
- Modify: `src/components/MoveDialog.ts`

- [ ] **Step 1: 读全文确认两函数差异**

Run: `cat src/components/MoveDialog.ts`
Expected: 确认 `openMoveDialog`（约 15-149）与 `openFolderMoveDialog`（约 165-303）的共同结构（overlay/dialog、close、onKey、targetName、render→renderNode、search.oninput 的 focus/setSelectionRange、.move-arrow/.move-cancel/.move-confirm 绑定、overlay 点击关闭、末尾挂载）与 4 处差异（禁选谓词、标题、tag 文案、confirm 回调）。

- [ ] **Step 2: 提取公共弹窗 openTreeMoveDialog**

新增内部函数（不 export，仅本文件用）：
```typescript
interface TreeMoveOpts {
  tree: TreeNode;
  title: string;                       // 弹窗标题（含 HTML，如 移动「X」到…）
  isDisabled: (path: string) => boolean; // 禁选谓词（当前位置/虚拟根/自身子树）
  nodeTag?: (path: string) => string;   // 某节点的标签文案（当前位置/移动中），返回空串则无
  confirmLabel: (targetName: string) => string; // 确认按钮文案
  onConfirm: (selectedPath: string) => Promise<void>; // 确认回调（内部做 api 调用）
}
function openTreeMoveDialog(opts: TreeMoveOpts): void {
  // 把两函数共同的 overlay/dialog/close/onKey/targetName/render/renderNode/search/
  // 事件绑定/挂载逻辑搬进来，差异点用 opts 的字段替代：
  //   - renderNode 里 isCur/disabled → opts.isDisabled(path)；tag → opts.nodeTag?.(path)
  //   - 标题 → opts.title；确认按钮文案 → opts.confirmLabel(targetName())
  //   - 点击选中禁选 → if (opts.isDisabled(p)) return
  //   - confirm onclick → await opts.onConfirm(selected)（含 try/catch/showToast/close/刷新）
}
```
保持渲染/搜索/交互逻辑与原来逐行等价。

- [ ] **Step 3: 两导出函数改为薄封装**

- `openMoveDialog(item, tree, kind, onMoved, crossCategory?)`：构造 opts 调 openTreeMoveDialog。isDisabled = 虚拟根 || 当前位置（currentPath）；title = 移动「displayTitle(item.title)」到…；confirm 里按 kind + crossCategory 调 mediaUpdate/comicUpdate/gameUpdate（保留原 splitCategoryPath 解析逻辑），成功 showToast + onMoved。
- `openFolderMoveDialog(opts)`：isDisabled = 虚拟根 || 自身子树（isSelfOrDesc）；title = 移动文件夹「folderName」到…；nodeTag 给自身标"移动中"；confirm 调 moveFolder（保留 splitCategoryPath 解析），成功 showToast + onMoved。
- 保持两个导出函数的对外签名不变（调用方 MediaLibraryView/FolderView 不用改）。

- [ ] **Step 4: 类型检查 + 构建**

Run: `npx tsc --noEmit && echo tsc-ok; npm run build 2>&1 | tail -1`
Expected: tsc-ok + 构建成功。

- [ ] **Step 5: 真机回归验证**

`npm run tauri:dev`，逐一确认行为与重构前一致：
- 影视单条目移动：右键视频→移动→合一树→跨分类移动成功、当前位置禁选、刷新。
- 漫画/游戏单条目移动：本分类内移动正常。
- 文件夹移动：右键文件夹→移动→合一树→移到目标、自身及子树禁选（灰显不可点）、跨分类成功。
- 搜索框过滤、展开/折叠、Esc/遮罩关闭均正常。

- [ ] **Step 6: 提交**
```bash
git add src/components/MoveDialog.ts
git commit -m "refactor(move): 提取 openTreeMoveDialog 消除 MoveDialog 两函数 ~80% 重复

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>"
```

---

## Task 4: 端到端验证

- [ ] **Step 1: 全套编译测试**

Run: `cd src-tauri && cargo test 2>&1 | grep "test result:" | head -1; cargo build 2>&1 | grep -iE "error|warning" || echo clean; cd .. && npx tsc --noEmit && echo tsc-ok; npm run build 2>&1 | tail -1`
Expected: 测试全过 + clean + tsc-ok + 构建成功。

- [ ] **Step 2: 确认清理生效**

Run: `grep -rn "\[DIAG\]\|fmp4_args\|pub mod cover\|\"grid\"" src-tauri/src src/lib/icons.ts 2>/dev/null; ls src-tauri/src/library/cover.rs 2>&1`
Expected: DIAG/fmp4_args/pub mod cover/grid 均无（cover.rs 不存在）。

- [ ] **Step 3: 工作树确认（只含本次代码变更 + 既有字幕变动）**

Run: `git status --short | grep -v "docs/subtitles" ; git log --oneline -4`
Expected: 除 docs/subtitles 的字幕变动外，工作树干净；3 个清理提交在列。字幕变动仍未提交（本次不碰，符合预期）。
