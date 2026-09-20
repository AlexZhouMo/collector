# 代码清理重构（第二轮：清冗余 + MoveDialog 去重）

日期：2026-09-20

## Context（为什么做这件事）

项目经历多轮功能开发（视频提速、缓存管理、文件夹/条目移动、可编辑路径、系统数据整理等）后，做一次逐文件代码清理，删除冗余/废弃/死代码/临时物/失效配置。

两个并行 Explore 代理（前端 TS、后端 Rust）已逐文件盘点。**整体质量高**：后端 cargo 零 warning、前端 tsconfig noUnusedLocals 拦截、无 orphan 模块、无 TODO/FIXME、无注释代码、无未用依赖、无废弃配置、35 命令与 34 api 方法各有调用方。真正的冗余集中且明确。

**范围外**：工作树里 440 项 `docs/subtitles/` 字幕目录重组变动（数据位移，非代码冗余）——本次不碰，保持现状。

## 已确认的决策

- 做 **A 类零风险清理** + **MoveDialog 去重**。
- 不做结构重构（scanner.rs 重命名、PlayerView/normalizeStore 拆分）。
- **保留** comic/game 的 category 预留列（为"三表合并"预留，当前不读）。

## A. 零风险清理

### A1. 删调试残留
- `src-tauri/src/player/mod.rs:190-191`：删除 `[DIAG worker-end]` 的 `eprintln!`（视频调试遗留，全仓唯一 `[DIAG]`；其余 eprintln 是有意的迁移/错误日志，保留）。

### A2. 删空模块 cover.rs
- 删 `src-tauri/src/library/cover.rs`（5 行纯注释、无代码，注释自述逻辑已挪到 lib.rs）+ 删 `src-tauri/src/library/mod.rs` 的 `pub mod cover;` 声明。确认无 `cover::` 引用。

### A3. 删死图标 grid
- `src/lib/icons.ts`：删 `IconName` 联合里的 `"grid"` + `PATHS.grid` 定义。全 src 无 `icon("grid")` 调用（CSS 的 display:grid 无关）。

### A4. 订正失效注释
- `src-tauri/src/player/transcode.rs`（模块头 1、5-6 行、`fmp4_args` 相关注释）+ `player/mod.rs:1`：现状是"普通 MP4 remux、`-c copy`、moov 留尾（不加 faststart）"，但注释仍描述已废弃的 fragmented MP4/empty_moov/faststart 方案，矛盾。改注释反映现状。**函数名 `fmp4_args` 名不副实**（非 fragmented），一并改为 `remux_args`（同步改调用处 + 测试名）。
- `src-tauri/src/poster/image_proc.rs:54`：注释说命名 `tmdb_<hash>.jpg`，实际有 prefix 参数（tmdb_/cover_ 两用），改为 `<prefix><hash>.jpg`。
- `src/assets/fonts/README.md`：说字体用于 JASSUB/`defaultFont`，实际是 SubtitlesOctopus/`fallbackFont`（SubtitleRenderer 明确弃用 JASSUB），改 README 描述。字体文件在用，勿删。

### A5. 收敛过度 export（去掉无外部消费者的 export）
- `src/lib/normalizeStore.ts`：`renderPosterTable`（仅同文件用）降为私有；类型 `TaskStatus`/`TaskState` 去 export（文件内用）。
- `src/components/CoverCropper.ts`：`screenRectToImageRect`/`ScreenRect` 去 export（仅内部用；注释"便于单测"失效——项目无测试框架，一并删注释或改）。
- `src/lib/ipc.ts`：`SubIssue`/`FetchReport`/`DbResetResult`/`MissingCover`/`CleanCoversResult` 去 export（均在本文件内作 invoke 返回/嵌套类型用，无外部 import）。
- 注：这些去 export 后若 tsc 报"声明但未使用"需保留必要的（作嵌套类型仍需存在，只是不导出）。以 tsc 通过为准。

## B. MoveDialog 去重

`src/components/MoveDialog.ts` 的 `openMoveDialog`（条目移动，约 15-149）与 `openFolderMoveDialog`（文件夹移动，约 165-303）**约 80% 结构重复**：overlay/dialog 搭建、close、onKey、targetName、render→renderNode、search.oninput（含 focus/setSelectionRange）、.move-arrow/.move-cancel/.move-confirm 绑定、overlay 点击关闭、末尾挂载——逐字重复。

差异仅：① 禁选判定（条目版 isCurrent；文件夹版 isDisabled/自身子树）② 标题文案 ③ tag 文案 ④ confirm 分支（mediaUpdate/comicUpdate/gameUpdate vs moveFolder）。

**方案**：提取一个内部公共函数（如 `openTreeMoveDialog(opts)`），参数化上述 4 个差异点（禁选谓词、标题、tag 文案、确认回调），两个导出函数（openMoveDialog/openFolderMoveDialog）改为薄封装调它。预计减 ~100 行。**行为必须完全不变**——需真机回归验证条目移动、文件夹移动（含跨分类、防移进自身、漫画/游戏）。

## 验证

- 后端：`cargo test`（含 maintenance/rename/move 等现有测试全过）、`cargo build` clean。
- 前端：`npx tsc --noEmit` clean、`npm run build` 成功。
- 运行：`npm run tauri:dev` 真机验证——移动功能（条目+文件夹、影视跨分类、漫画/游戏、防移进自身）行为与重构前一致；图标显示正常（grid 删除不影响任何在用图标）。

## 范围外（不做）
- 440 项字幕目录重组变动（数据，非代码）。
- scanner.rs 重命名、PlayerView/normalizeStore 拆分（结构重构）。
- comic/game category 预留列（保留）。
