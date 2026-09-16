# 代码优化重构与冗余清理设计

日期：2026-09-17

## Context（为什么做这件事）

用户要求对整个项目做一次系统性优化重构：逐个文件检查，删除冗余/废弃文件、空文件夹、死代码、无效配置、临时/缓存文件等。

同时，项目里有一个**未完成的视频卡顿修复**任务（改用 fragmented MP4 边转边播 + HTTP server 8MB Range 切块），代码中残留了临时诊断日志和一个测试文件（`public/fmp4test.html`）。用户决定把该修复的收尾（清理诊断日志）并入本次清理；切块修复本身保留（已用 curl 验证 HTTP 响应正确），但"卡顿是否真正解决"尚未在真实应用里验证。

三个并行 Explore 代理已逐文件盘点前端 TS、后端 Rust、配置/脚本/资源，所有删除项均经 grep/cargo 交叉验证。整体代码质量高：后端零 `cargo` warning、无 TODO/dbg 残留；前端无 console.log、无注释代码块。

**目标**：在不改变任何功能的前提下，清理冗余、合并重复、拆分过大文件，改善可维护性并释放磁盘。

## 已确认的关键决策

- 清理范围：代码冗余（入库）+ 本地临时产物 + 结构重构，全做。
- `docs/superpowers/plans/` 与 `specs/` 历史文档：**保留**。
- 废弃脚本 `poster_regression.py` / `poster_baseline.json` / `test-moveTreeFilter.ts`：**删除**（推翻此前"保留"的决定）。
- 入库大数据：`docs/subtitles/`(160MB) 与 `seed/subtitles/`(141MB) 两套内容不同、用途不同，**保留不动**；只清 `icons/` 里用不上的图标。
- 视频修复：与本次清理**合并一起做**，诊断日志作为清理对象。切块修复保留。
- 结构重构范围：后端重复合并+大文件拆分（2a/2b）+ 前端三视图去重（2c）；**PlayerView 拆分（2d）暂缓**，等视频卡顿确认解决后单独做。

## 第 1 层：低风险清理（不改功能）

### 1a. 本地临时物（未入库，仅释放磁盘 ~160MB+）
- `dist/`、`src-tauri/bin/ffmpeg*`（可重新 fetch）、`src-tauri/gen/schemas/`、`.superpowers/`（含残留 server.log）

### 1b. 视频修复收尾
- 删 `public/fmp4test.html`，删 `.claude/launch.json` 的 `fmp4-test` 配置项。
- 清理临时诊断：
  - `src/views/PlayerView.ts`：正常流程 `console.error` 打点（`[player] init subtitle @`、`[player] transcode-progress`、`[player] playerOpen ->`）。保留合理错误日志（`video error`、`playerOpen failed`、`setFullscreen failed`）。
  - `src/components/SubtitleRenderer.ts`：诊断打点（`onReady` 里 `octopus ready, ctor blocked`、`ctor returned in ... ms`）。保留合理错误日志。
  - `src-tauri/src/player/mod.rs`：`[player_open]`/`[transcode]` 前缀 `eprintln!` 及为诊断加的临时字段。
  - `src-tauri/src/player/httpserver.rs`：`[httpsrv]` 前缀 `eprintln!`（FULL/GROWING/WAIT-TIMEOUT）。
- **保留**：httpserver 的 `MAX_RANGE_CHUNK` 8MB 切块逻辑、fragmented MP4 边转边播全部实现。

### 1c. 入库死代码/冗余（已 grep 三重验证）
- 前端：删 `src/lib/ipc.ts` 的 `api.scanRoot`/`scanVideos`/`importCover`（前端无调用）；同步删后端 `lib.rs` 对应 command 定义与 `invoke_handler` 注册（`scan_root`、`scan_videos_all`、`import_cover`）。删 `src/lib/spreads.ts` 冗余 `export type { PageInfo }`（唯一消费者只 import `buildSpreads`）。
- 后端：
  - 删 `src-tauri/src/normalize/comic_pack.rs` 的 `pack_comic_dir`（`#[allow(dead_code)]` 掩盖，仅自身 tests 调用）及其两个专属测试。
  - 删 `src-tauri/src/lib.rs:438` 孤立失效 doc 注释（描述已重构掉的 import_cover 包装）。
  - 订正 3 处失效注释：`library/mod.rs` 的 subtitle_path 列描述（media 现已无该列）、`delete_item` 的"comic/game 暂无删除需求"（实际已实现）、`poster/mod.rs:105` 的 `table` 参数描述。

### 1d. 废弃脚本
- 删 `scripts/poster_regression.py`、`scripts/poster_baseline.json`、`scripts/test-moveTreeFilter.ts`。

### 1e. 无用图标
- 删 `src-tauri/icons/` 中未被 `tauri.conf.json` 引用的：9 个 `Square*Logo.png`、`StoreLogo.png`、`64x64.png`、`icon.png`。保留 `32x32.png`、`128x128.png`、`128x128@2x.png`、`icon.icns`、`icon.ico`。

### 1f. 空目录
- 删 `docs/subtitles/剧集/美剧/权利的游戏/第{1..8}季` 8 个空季目录。

## 第 2 层：结构重构（行为不变，靠测试+运行验证）

### 2a. 后端重复逻辑合并
- `library/mod.rs`：`list_items` 的 Comic/Game 近乎相同分支 → 提取 `list_kind_rows(conn, table)`；`insert_one_tx`/`create_item` 的 comic/game 重复 SQL 收拢。
- `poster/tmdb.rs`：`search_detailed` 与 `search` 主体重复 → 抽 `search_raw(...)` 复用 endpoint/year/URL/请求。
- `poster/mod.rs`：`update_cover_path` 的 `table` 参数恒传 `"media"` → 简化（去投机性通用化）。

### 2b. 后端大文件拆分
- `lib.rs`（897 行）：`migrate_subtitles_to_plain`、`migrate_covers_to_subdirs`、`release_seed_if_empty` 拆到独立 `migrate.rs`，`lib.rs` 只留 command 与 `run()`。
- `lib.rs` 的 `fetch_posters`（126 行）：内部 `fetch_cover`/`suggest` 闭包提为 `poster` 模块具名函数。

### 2c. 前端三视图去重
- `VideoView.ts`/`ComicView.ts`/`GameView.ts` 同构部分（`ViewMode` 类型、`video-bar`/`view-toggle` 工具栏 HTML、`updateAddBtn()`、`mode/folderPath/treeSelected` 状态、删除确认块）→ 提取公共工厂 `MediaLibraryView(kind, cats?)` 或共享工具栏组件。

### 暂缓（不在本次范围）
- 2d. `PlayerView.ts` 拆分（全屏控制器/进度状态机/快捷键）——等视频卡顿真实验证解决后单独做。
- `library/scanner.rs` 与 `comic/mod.rs` 的卷号解析 helper 合并、`normalizeStore.ts` 渲染逻辑拆分等低优先项——按需再议。

## 执行分批与验证

每批独立提交、可回滚，提交前跑全套验证。

- **批次一（第 1 层全部）**：验证 `cd src-tauri && cargo test`（现有 119+ 测试全过）、`cargo build`、`npx tsc --noEmit`、`npm run build`。
- **批次二（2a/2b）**：`cargo test` 全过（重构不改行为，测试是护栏）+ `cargo build`。
- **批次三（2c）**：`npx tsc --noEmit` + `npm run tauri:dev` 实际运行，逐个点开视频/漫画/游戏三库，确认工具栏、folder/tree 切换、新增/编辑/移动/删除均正常。

**风险控制**：删除的入库代码 git 历史可找回；后端重构靠现有测试兜底；前端改动实际运行验证。视频卡顿的真实验证与 PlayerView 拆分留作后续单独任务。

## 验证清单（端到端）

1. 后端：`cd src-tauri && cargo test && cargo build`（零 warning、全绿）。
2. 前端：`npx tsc --noEmit`（无类型错误）、`npm run build`（构建成功）。
3. 运行：`npm run tauri:dev` 启动，抽查视频播放（含字幕）、漫画阅读、三库工具栏与增删改。
4. git：每批次一次提交，工作树最终 `git status` 干净（除本地 gitignore 临时物）。
