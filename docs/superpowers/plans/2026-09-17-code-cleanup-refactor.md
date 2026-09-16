# 代码优化重构与冗余清理 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 清理项目冗余（本地临时物、入库死代码、失效注释、废弃脚本、无用图标、空目录）并做低风险结构重构（后端重复合并+lib.rs 拆分、前端三视图去重），不改变任何功能。

**Architecture:** 分三批执行，每批独立提交、独立验证。批次一为纯清理（删除+注释订正）；批次二为后端重构（有 cargo test 兜底）；批次三为前端三视图去重（tsc + 实际运行验证）。视频卡顿修复的临时诊断在批次一清除，切块修复本身保留；PlayerView 拆分不在本计划范围。

**Tech Stack:** Rust (Tauri 2, rusqlite, tiny_http)、TypeScript (Vite)、cargo test、npm。

**验证命令基线**（各批次反复用到）：
- 后端：`cd /Users/zhoumo/Documents/Claude/collector/src-tauri && cargo test && cargo build`
- 前端类型：`cd /Users/zhoumo/Documents/Claude/collector && npx tsc --noEmit`
- 前端构建：`cd /Users/zhoumo/Documents/Claude/collector && npm run build`

---

## 批次一：低风险清理

### Task 1: 删除本地临时物（释放磁盘，不入库）

**Files:**
- Delete: `dist/`、`src-tauri/bin/ffmpeg-aarch64-apple-darwin`、`src-tauri/bin/ffprobe-aarch64-apple-darwin`、`src-tauri/gen/schemas/`、`.superpowers/`

- [ ] **Step 1: 确认这些都未被 git 跟踪**

Run:
```bash
cd /Users/zhoumo/Documents/Claude/collector
git ls-files dist/ src-tauri/gen/ .superpowers/ src-tauri/bin/ffmpeg-aarch64-apple-darwin src-tauri/bin/ffprobe-aarch64-apple-darwin
```
Expected: 空输出（全部未跟踪；`src-tauri/bin/.gitkeep` 若被跟踪属正常，不在删除列表）。

- [ ] **Step 2: 删除临时物**

Run:
```bash
cd /Users/zhoumo/Documents/Claude/collector
rm -rf dist src-tauri/gen/schemas .superpowers
rm -f src-tauri/bin/ffmpeg-aarch64-apple-darwin src-tauri/bin/ffprobe-aarch64-apple-darwin
```

- [ ] **Step 3: 确认工作树未受影响（无 git 变更）**

Run: `git status --short`
Expected: 不出现上述被删路径（因未跟踪）；只显示此前视频修复的已改文件与 `public/fmp4test.html`。

（本 Task 无提交——纯本地清理，无版本库变更。）

---

### Task 2: 删除视频修复的临时测试文件与 launch 配置

**Files:**
- Delete: `public/fmp4test.html`
- Modify: `.claude/launch.json`（移除 `fmp4-test` 配置项）

- [ ] **Step 1: 删除测试页**

Run:
```bash
cd /Users/zhoumo/Documents/Claude/collector
rm -f public/fmp4test.html
```

- [ ] **Step 2: 还原 launch.json 为仅 collector-dev**

将 `.claude/launch.json` 整体替换为：
```json
{
  "version": "0.0.1",
  "configurations": [
    {
      "name": "collector-dev",
      "runtimeExecutable": "npm",
      "runtimeArgs": ["run", "dev"],
      "port": 1420,
      "url": "http://localhost:1420"
    }
  ]
}
```

- [ ] **Step 3: 提交**

```bash
cd /Users/zhoumo/Documents/Claude/collector
git add .claude/launch.json
git rm --cached public/fmp4test.html 2>/dev/null; true
git commit -m "chore: 移除视频排障临时测试页与 launch 配置

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>"
```
说明：`public/fmp4test.html` 未入库，`git rm --cached` 可能报未跟踪，`; true` 兜底；本提交主要记录 launch.json 变更。

---

### Task 3: 清除后端播放模块的临时诊断 eprintln

**Files:**
- Modify: `src-tauri/src/player/mod.rs`（删 145、166、172、184 行的 eprintln）
- Modify: `src-tauri/src/player/httpserver.rs`（删 122、126、201、207 行的 eprintln 及相关计时变量）

- [ ] **Step 1: 删 mod.rs 的诊断**

删除 `src-tauri/src/player/mod.rs` 中这 4 行 eprintln（不删其他逻辑）：
- `eprintln!("[player_open] progressive={progressive} epoch={epoch} src={src} dur={duration:.1}");`
- `eprintln!("[transcode] worker start epoch={epoch} part={part:?}");`
- `eprintln!("[transcode] epoch={epoch} ok_seconds={ok_seconds:.2}");`
- `eprintln!("[transcode] worker end epoch={epoch} saw_end={saw_end} success={success} ok_seconds={ok_seconds:.2}");`

- [ ] **Step 2: 删 httpserver.rs 的诊断及计时变量**

在 `src-tauri/src/player/httpserver.rs`：
- 删 `eprintln!("[httpsrv] GROWING url={url} range={range_hdr:?}");`（handle 的 growing 分支内，删后该分支只剩 `serve_growing(...)` 调用）
- 删 `eprintln!("[httpsrv] FULL url={url} range={range_hdr:?}");`
- 在 `serve_growing` 中，将带计时诊断的等待块还原为无诊断版本。把这段：
```rust
    // 等待文件增长到 start 之后（ffmpeg 写到该处）。超时 416。
    let wait_start = Instant::now();
    let size = match wait_for_offset(path, start) {
        Some(s) => s,
        None => {
            eprintln!("[httpsrv] GROWING start={start} WAIT-TIMEOUT after {:?} -> 416", wait_start.elapsed());
            let _ = request.respond(Response::empty(StatusCode(416)));
            return;
        }
    };
    if wait_start.elapsed().as_millis() > 200 {
        eprintln!("[httpsrv] GROWING start={start} waited {:?} before size={size}", wait_start.elapsed());
    }
```
替换为：
```rust
    // 等待文件增长到 start 之后（ffmpeg 写到该处）。超时 416。
    let size = match wait_for_offset(path, start) {
        Some(s) => s,
        None => {
            let _ = request.respond(Response::empty(StatusCode(416)));
            return;
        }
    };
```

- [ ] **Step 3: 确认无遗留诊断且 Instant 仍被使用**

Run:
```bash
cd /Users/zhoumo/Documents/Claude/collector
grep -rn 'eprintln!("\[httpsrv\]\|eprintln!("\[player_open\]\|eprintln!("\[transcode\]' src-tauri/src/player/
grep -n "Instant" src-tauri/src/player/httpserver.rs
```
Expected: 第一条空输出（诊断已清）；第二条仍有 `use std::time::{Duration, Instant}` 与 `wait_for_offset` 内的 `Instant::now()`（Instant 仍在用，import 不必删）。若 grep 显示 Instant 只剩 import，则一并把 import 改为 `use std::time::Duration;`。

- [ ] **Step 4: 编译验证**

Run: `cd /Users/zhoumo/Documents/Claude/collector/src-tauri && cargo build 2>&1 | grep -iE "warning|error" || echo "clean"`
Expected: `clean`（无 warning/error；尤其无 unused import/unused variable）。

- [ ] **Step 5: 提交**

```bash
cd /Users/zhoumo/Documents/Claude/collector
git add src-tauri/src/player/mod.rs src-tauri/src/player/httpserver.rs
git commit -m "chore(player): 清除边转边播排障的临时诊断日志

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>"
```

---

### Task 4: 清除前端播放模块的临时诊断 console.error

**Files:**
- Modify: `src/views/PlayerView.ts`（删 84、133、141 行诊断）
- Modify: `src/components/SubtitleRenderer.ts`（删 66 的 onReady 诊断、69 的 ctor 计时；去掉 t0）

- [ ] **Step 1: 删 PlayerView.ts 的 3 处诊断打点**

删除这 3 行（保留其所在函数的其他逻辑）：
- `console.error("[player] init subtitle @", performance.now().toFixed(0));`
- `console.error("[player] transcode-progress", JSON.stringify(e.payload), "want epoch", wantEpoch);`
- `console.error("[player] playerOpen ->", JSON.stringify(info));`

保留合理错误日志：`video error`、`playerOpen failed`、`setFullscreen failed`。

- [ ] **Step 2: 还原 SubtitleRenderer.init 去掉诊断**

将 `src/components/SubtitleRenderer.ts` 的 `init` 方法体（含 t0 计时与 onReady/ctor 诊断）替换为无诊断版本：
```typescript
  private init(video: HTMLVideoElement, subContent: string): void {
    try {
      const Ctor = SubtitlesOctopus as unknown as OctopusCtor;
      this.instance = new Ctor({
        video,
        subContent,
        workerUrl: WORKER_URL,
        legacyWorkerUrl: LEGACY_WORKER_URL,
        // 兜底字体：字幕指名字体缺失时用它（中文不方框）
        fonts: [CJK_FONT_URL],
        fallbackFont: CJK_FONT_URL,
        onError: (e) => console.error("[subtitle] SubtitlesOctopus error", e),
      });
    } catch (e) {
      console.error("[subtitle] SubtitlesOctopus init failed", e);
      this.instance = null;
    }
  }
```

- [ ] **Step 3: 确认无遗留诊断**

Run:
```bash
cd /Users/zhoumo/Documents/Claude/collector
grep -rn 'init subtitle @\|transcode-progress"\|playerOpen ->\|octopus ready\|ctor returned\|ctor blocked' src/
```
Expected: 空输出。

- [ ] **Step 4: 类型检查**

Run: `cd /Users/zhoumo/Documents/Claude/collector && npx tsc --noEmit`
Expected: 无输出（exit 0）。

- [ ] **Step 5: 提交**

```bash
cd /Users/zhoumo/Documents/Claude/collector
git add src/views/PlayerView.ts src/components/SubtitleRenderer.ts
git commit -m "chore(player): 清除前端边转边播排障的临时诊断日志

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>"
```

---

### Task 5: 删除未使用的 IPC 封装与对应后端 command

**Files:**
- Modify: `src/lib/ipc.ts`（删 27、28、64 行）
- Modify: `src-tauri/src/lib.rs`（删 `scan_root`/`scan_videos_all`/`import_cover` 定义与 invoke_handler 注册）

- [ ] **Step 1: 复核这三个封装全项目无调用**

Run:
```bash
cd /Users/zhoumo/Documents/Claude/collector
grep -rn "scanRoot\|scanVideos\|api\.importCover\b" src/ | grep -v "src/lib/ipc.ts"
grep -rn "scan_root\|scan_videos_all\|import_cover\b" src-tauri/src/ | grep -v "lib.rs"
```
Expected: 第一条空（前端无调用）；第二条只可能出现在 `library/cover.rs` 的 `import_cover`（那是 library::cover::import_cover，不同函数，保留）；确认 lib.rs 的 command 无外部调用。

- [ ] **Step 2: 删前端三行封装**

删除 `src/lib/ipc.ts` 中：
- `scanRoot: (kind: string) => invoke<number>("scan_root", { kind }),`
- `scanVideos: () => invoke<number>("scan_videos_all"),`
- `importCover: (srcImage: string, kind: string) => invoke<string>("import_cover", { srcImage, kind }),`

- [ ] **Step 3: 删后端三个 command 函数定义**

在 `src-tauri/src/lib.rs` 删除三个 `#[tauri::command]` 函数：`scan_root`（约 28-51 行区块，含其 `#[tauri::command]` 属性行）、`scan_videos_all`（约 53 行起的整个函数）、`import_cover`（约 450 行起的整个函数，注意别删到相邻的 `cover_subdir`）。删除每个函数时连同其上方的 `#[tauri::command(...)]` 属性与文档注释。

- [ ] **Step 4: 删 invoke_handler 里的三处注册**

在 `src-tauri/src/lib.rs` 的 `tauri::generate_handler![...]` 里删除这三行：`scan_root,`、`scan_videos_all,`、`import_cover,`。

- [ ] **Step 5: 编译 + 类型检查**

Run:
```bash
cd /Users/zhoumo/Documents/Claude/collector/src-tauri && cargo build 2>&1 | grep -iE "warning|error" || echo "rust clean"
cd /Users/zhoumo/Documents/Claude/collector && npx tsc --noEmit && echo "ts clean"
```
Expected: `rust clean` + `ts clean`（无未使用 import 等 warning；若删 `import_cover` 后 `library::cover` 或某 use 变未用，一并清理该 use）。

- [ ] **Step 6: 提交**

```bash
cd /Users/zhoumo/Documents/Claude/collector
git add src/lib/ipc.ts src-tauri/src/lib.rs
git commit -m "refactor: 删除未使用的 scan_root/scan_videos_all/import_cover 命令与前端封装

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>"
```

---

### Task 6: 删除 spreads.ts 冗余 re-export

**Files:**
- Modify: `src/lib/spreads.ts`（删第 3 行 re-export；第 4 行 Spread 去 export 或保留——见步骤）

- [ ] **Step 1: 确认无人从 spreads 引入 PageInfo/Spread**

Run:
```bash
cd /Users/zhoumo/Documents/Claude/collector
grep -rn 'from "\.\./lib/spreads"\|from "\./spreads"' src/
```
Expected: 只有 `ComicReaderView.ts` 一处，且 `import { buildSpreads }`（不含 PageInfo/Spread）。

- [ ] **Step 2: 删冗余 re-export**

删除 `src/lib/spreads.ts` 第 3 行：`export type { PageInfo };`
保留第 4 行 `Spread`（它是 `buildSpreads` 的返回类型，仍需可见；去掉 export 也可，但保留 export 无害，为最小改动只删第 3 行）。

- [ ] **Step 3: 类型检查**

Run: `cd /Users/zhoumo/Documents/Claude/collector && npx tsc --noEmit`
Expected: 无输出（`PageInfo` 仍在文件内被 `isWide`/`buildSpreads` 使用，import 保留）。

- [ ] **Step 4: 提交**

```bash
cd /Users/zhoumo/Documents/Claude/collector
git add src/lib/spreads.ts
git commit -m "refactor: 删除 spreads.ts 无消费者的 PageInfo re-export

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>"
```

---

### Task 7: 删除死函数 pack_comic_dir，改写其相关测试

**Files:**
- Modify: `src-tauri/src/normalize/comic_pack.rs`（删 103-122 的 `pack_comic_dir` + `#[allow(dead_code)]`；删测试 `packs_images_into_renamed_jpg_zip`；改写 `natural_sort_orders_unpadded_numbers` 为直接测 natural_key）

- [ ] **Step 1: 复核 pack_comic_dir 仅测试引用、生产路径已覆盖其行为**

Run:
```bash
cd /Users/zhoumo/Documents/Claude/collector
grep -rn "pack_comic_dir" src-tauri/src/
grep -n "natural_key\|is_system_junk\|pack_images_to_zip" src-tauri/src/normalize/comic_archive.rs
```
Expected: `pack_comic_dir` 只在 comic_pack.rs 自身出现（定义+2 测试）；comic_archive.rs 生产路径已用 natural_key 排序 + junk 过滤 + pack_images_to_zip（行为已覆盖）。

- [ ] **Step 2: 删除 pack_comic_dir 函数**

删除 `src-tauri/src/normalize/comic_pack.rs` 的第 103-122 行（含 `#[allow(dead_code)]` 属性与整个 `pub fn pack_comic_dir` 函数体）。

- [ ] **Step 3: 删除 packs_images_into_renamed_jpg_zip 测试**

删除 `mod tests` 中的 `packs_images_into_renamed_jpg_zip` 测试（其覆盖的 junk 过滤+重命名已在 comic_archive.rs 测试中）。

- [ ] **Step 4: 改写 natural_sort_orders_unpadded_numbers 为直接测 natural_key**

将该测试整体替换为直接验证自然排序的单元测试（不再依赖 pack_comic_dir）：
```rust
    #[test]
    fn natural_sort_orders_unpadded_numbers() {
        // 字典序会把 "10" 排在 "2" 前；natural_key 应让 1 < 2 < 10。
        let mut names = vec!["10.png", "1.png", "2.png"];
        names.sort_by(|a, b| natural_key(a).cmp(&natural_key(b)));
        assert_eq!(names, vec!["1.png", "2.png", "10.png"]);
    }
```

- [ ] **Step 5: 运行测试确认全绿**

Run: `cd /Users/zhoumo/Documents/Claude/collector/src-tauri && cargo test comic_pack 2>&1 | tail -15`
Expected: 该模块测试全部 PASS（含改写后的 natural_sort、保留的 pack_images_to_zip_uses_namer / on_image_called_once_per_image / parallel_* 测试）。

- [ ] **Step 6: 全量编译 + 测试（确认无 dead_code warning、无破坏）**

Run: `cd /Users/zhoumo/Documents/Claude/collector/src-tauri && cargo build 2>&1 | grep -iE "warning|error" || echo "clean"; cargo test 2>&1 | tail -5`
Expected: `clean` + 全部测试 PASS。

- [ ] **Step 7: 提交**

```bash
cd /Users/zhoumo/Documents/Claude/collector
git add src-tauri/src/normalize/comic_pack.rs
git commit -m "refactor: 删除死函数 pack_comic_dir，自然排序改为直接测 natural_key

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>"
```

---

### Task 8: 订正失效注释、删除孤立注释

**Files:**
- Modify: `src-tauri/src/lib.rs:438`（删孤立 doc 注释）
- Modify: `src-tauri/src/library/mod.rs`（订正 subtitle_path 与 delete 注释）
- Modify: `src/views/PlayerView.ts:56`（订正 remux 失效注释）

> 注：`poster/mod.rs:105` 的 `update_cover_path` table 参数注释**不在本 Task 处理**，交由批次二 Task 14（该任务会删掉 table 参数并同步定稿注释），避免两处改同一行冲突。

- [ ] **Step 1: 删 lib.rs 的孤立注释**

删除 `src-tauri/src/lib.rs` 中这一孤立行（它下方隔空行才是 `cover_subdir` 的注释，描述的是已重构掉的 import_cover 包装）：
`/// 把 src 图拷到 <app_data>/covers，返回相对路径 covers/xxx（供前端填 coverPath 存库）。`

- [ ] **Step 2: 订正 library/mod.rs 的 subtitle_path 注释**

在 `src-tauri/src/library/mod.rs` 第 45、121-122 附近，找到声称"media 表含 subtitle_path 列"的注释。media 表现已无该列（迁移已 DROP），改为准确描述：字幕改用明文路径存储（见 subtitle 迁移），三张表均无 subtitle_path 列。用一句准确注释替换原失效表述。

- [ ] **Step 3: 订正 library/mod.rs 的 delete_item 注释**

第 112 行 `delete_item` 附近注释"comic/game 暂无删除需求"已过时（comic_delete/game_delete 已实现）。改为准确描述当前 delete_item 的适用范围（media 删除；comic/game 走 delete_media_kind_row）。

- [ ] **Step 4: 订正 PlayerView.ts:56 的 remux 注释**

`src/views/PlayerView.ts` 第 56 行附近注释 `// remux 成临时 mp4（秒级），再用 asset:// 播放本地文件` 与当前实现（边转边播/进度阈值起播/HTTP server Range）不符。替换为准确描述：经本地 HTTP server 播放缓存 mp4（完整产物秒开复用；未缓存则边转边播，进度阈值起播），加载/解码错误显示具体原因。

- [ ] **Step 5: 编译 + 类型检查**

Run:
```bash
cd /Users/zhoumo/Documents/Claude/collector/src-tauri && cargo build 2>&1 | grep -iE "warning|error" || echo "rust clean"
cd /Users/zhoumo/Documents/Claude/collector && npx tsc --noEmit && echo "ts clean"
```
Expected: `rust clean` + `ts clean`（注释改动不影响编译，此步确认没误删代码）。

- [ ] **Step 6: 提交**

```bash
cd /Users/zhoumo/Documents/Claude/collector
git add src-tauri/src/lib.rs src-tauri/src/library/mod.rs src/views/PlayerView.ts
git commit -m "docs: 订正失效注释、删除重构残留的孤立注释

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>"
```

---

### Task 9: 删除废弃脚本

**Files:**
- Delete: `scripts/poster_regression.py`、`scripts/poster_baseline.json`、`scripts/test-moveTreeFilter.ts`

- [ ] **Step 1: 复核未被 package.json/CI/源码引用**

Run:
```bash
cd /Users/zhoumo/Documents/Claude/collector
grep -rn "poster_regression\|poster_baseline\|test-moveTreeFilter" package.json .github/ src/ src-tauri/src/ 2>/dev/null
```
Expected: 空或仅历史 .md 文档提及（不影响功能）。确认 package.json scripts、CI workflow 均不调用它们。

- [ ] **Step 2: 删除三个废弃脚本**

Run:
```bash
cd /Users/zhoumo/Documents/Claude/collector
git rm scripts/poster_regression.py scripts/poster_baseline.json scripts/test-moveTreeFilter.ts
```

- [ ] **Step 3: 确认剩余脚本仍在用**

Run: `ls scripts/`
Expected: 剩 `fetch-ffmpeg.mjs`、`prepare-seed.sh`（二者被 package.json/CI 或 seed 生成流程引用，保留）。

- [ ] **Step 4: 提交**

```bash
cd /Users/zhoumo/Documents/Claude/collector
git commit -m "chore: 删除废弃的一次性脚本 poster_regression/baseline 与 moveTreeFilter 测试

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>"
```

---

### Task 10: 删除未使用的图标

**Files:**
- Delete: `src-tauri/icons/` 中未被 tauri.conf.json 引用的图标

- [ ] **Step 1: 确认 tauri.conf.json 实际引用的图标**

Run:
```bash
cd /Users/zhoumo/Documents/Claude/collector
grep -A12 '"icon"' src-tauri/tauri.conf.json
ls src-tauri/icons/
```
Expected: config 只引用 `32x32.png`、`128x128.png`、`128x128@2x.png`、`icon.icns`、`icon.ico`。

- [ ] **Step 2: 删除未引用图标**

Run:
```bash
cd /Users/zhoumo/Documents/Claude/collector
git rm src-tauri/icons/64x64.png src-tauri/icons/icon.png src-tauri/icons/StoreLogo.png src-tauri/icons/Square*Logo.png
```
说明：`Square*Logo.png` 是 Windows Store/UWP 磁贴（9 个），当前 NSIS 打包用不上。若某个文件名不存在导致 git rm 报错，逐个核对 `ls` 输出后删除实际存在的。

- [ ] **Step 3: 确认打包引用的图标仍在**

Run: `ls src-tauri/icons/`
Expected: 保留 `32x32.png`、`128x128.png`、`128x128@2x.png`、`icon.icns`、`icon.ico`。

- [ ] **Step 4: 构建验证（确认打包配置未坏）**

Run: `cd /Users/zhoumo/Documents/Claude/collector/src-tauri && cargo build 2>&1 | tail -3`
Expected: 编译成功（图标在 tauri build 打包阶段用，dev build 不阻塞；确认 config 引用的 5 个图标未被误删即可）。

- [ ] **Step 5: 提交**

```bash
cd /Users/zhoumo/Documents/Claude/collector
git commit -m "chore: 删除未被打包配置引用的图标（Store 磁贴/64x64/icon.png）

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>"
```

---

### Task 11: 删除空目录

**Files:**
- Delete: `docs/subtitles/剧集/美剧/权利的游戏/第{1..8}季` 8 个空目录

- [ ] **Step 1: 确认这些目录为空**

Run:
```bash
cd /Users/zhoumo/Documents/Claude/collector
find "docs/subtitles/剧集/美剧/权利的游戏" -type d -empty
```
Expected: 列出 8 个空季目录（第1季…第8季）。

- [ ] **Step 2: 删除空目录**

Run:
```bash
cd /Users/zhoumo/Documents/Claude/collector
find "docs/subtitles/剧集/美剧/权利的游戏" -type d -empty -delete
```
说明：git 不跟踪空目录，故此删除仅影响本地文件系统，无版本库变更。若删后"权利的游戏"父目录也空了，可一并删。

- [ ] **Step 3: 确认无 git 变更**

Run: `cd /Users/zhoumo/Documents/Claude/collector && git status --short`
Expected: 无因空目录产生的变更（本 Task 无提交）。

---

## 批次二：后端结构重构（cargo test 兜底）

### Task 12: 合并 library/mod.rs 的 Comic/Game 重复分支

**Files:**
- Modify: `src-tauri/src/library/mod.rs`（`list_items` 的 comic/game 分支提取为 `list_kind_rows`）

- [ ] **Step 1: 读现状，确认两分支差异仅表名**

Run: `sed -n '194,255p' src-tauri/src/library/mod.rs`
Expected: 看到 `list_items` 中 Comic 与 Game 两分支除表名 `comic`/`game` 外结构一致。

- [ ] **Step 2: 提取共享函数 list_kind_rows**

在 `src-tauri/src/library/mod.rs` 新增私有函数（放在 `list_items` 上方），把 comic/game 两分支共用的查询逻辑参数化为表名。函数签名：
```rust
fn list_kind_rows(conn: &rusqlite::Connection, table: &str) -> AppResult<Vec<MediaItem>> {
    // 用现有 comic/game 分支的 SELECT 逻辑，把表名替换为 {table}。
    // 注意：表名不能用 SQL 占位符绑定，需用 format! 拼接，且 table 只来自内部常量 "comic"/"game"（无注入风险）。
    // 具体 SQL 与列映射照搬原分支实现。
}
```
然后把 `list_items` 的 Comic/Game 两分支改为分别调用 `list_kind_rows(conn, "comic")` / `list_kind_rows(conn, "game")`。（Media 分支若结构不同则保持不变。）

- [ ] **Step 3: 运行 library 相关测试**

Run: `cd /Users/zhoumo/Documents/Claude/collector/src-tauri && cargo test library 2>&1 | tail -10`
Expected: 全部 PASS（行为不变）。

- [ ] **Step 4: 全量测试 + 编译**

Run: `cd /Users/zhoumo/Documents/Claude/collector/src-tauri && cargo test 2>&1 | tail -5 && cargo build 2>&1 | grep -iE "warning|error" || echo "clean"`
Expected: 测试全绿 + `clean`。

- [ ] **Step 5: 提交**

```bash
cd /Users/zhoumo/Documents/Claude/collector
git add src-tauri/src/library/mod.rs
git commit -m "refactor(library): 提取 list_kind_rows 合并 comic/game 查询重复分支

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>"
```

---

### Task 13: 合并 poster/tmdb.rs 的 search/search_detailed 重复

**Files:**
- Modify: `src-tauri/src/poster/tmdb.rs`（抽 `search_raw` 复用 endpoint/year/URL/请求）

- [ ] **Step 1: 读现状确认两函数主体重复**

Run: `sed -n '26,120p' src-tauri/src/poster/tmdb.rs`
Expected: `search_detailed`（26-57）与 `search`（86-119）主体重复：endpoint 选择、year_param、URL 拼接、请求、`pick_by_year`，只有末尾 `.map()` 出的结构体不同。

- [ ] **Step 2: 抽取 search_raw 返回原始 JSON 命中**

在 `src-tauri/src/poster/tmdb.rs` 新增私有函数，封装两者共用的"选 endpoint → 拼 year → 请求 → pick_by_year"，返回选中的原始 `serde_json::Value`：
```rust
fn search_raw(key: &str, title: &str, year: Option<u32>, is_tv: bool) -> Option<serde_json::Value> {
    // 照搬原 search/search_detailed 共有的 endpoint/year_param/URL/请求/pick_by_year 逻辑，
    // 返回选中的那条结果 JSON（Value），不做结构体映射。
}
```
然后把 `search` 与 `search_detailed` 改为：调用 `search_raw(...)` 拿到 Value 后，各自 `.map()` 成自己的返回结构（`search` → 现有返回类型，`search_detailed` → `TmdbDetail`）。

- [ ] **Step 3: 运行 poster/tmdb 相关测试**

Run: `cd /Users/zhoumo/Documents/Claude/collector/src-tauri && cargo test poster 2>&1 | tail -10`
Expected: 全部 PASS。若 tmdb 涉及网络的测试被 ignore，确认非网络的解析/pick_by_year 测试 PASS。

- [ ] **Step 4: 全量测试 + 编译**

Run: `cd /Users/zhoumo/Documents/Claude/collector/src-tauri && cargo test 2>&1 | tail -5 && cargo build 2>&1 | grep -iE "warning|error" || echo "clean"`
Expected: 测试全绿 + `clean`。

- [ ] **Step 5: 提交**

```bash
cd /Users/zhoumo/Documents/Claude/collector
git add src-tauri/src/poster/tmdb.rs
git commit -m "refactor(poster): 抽取 search_raw 消除 search/search_detailed 重复主体

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>"
```

---

### Task 14: 简化 poster/mod.rs 的 update_cover_path table 参数

**Files:**
- Modify: `src-tauri/src/poster/mod.rs`（`update_cover_path` 去掉恒为 "media" 的 table 参数）

- [ ] **Step 1: 确认唯一调用点恒传 media**

Run: `grep -n "update_cover_path" src-tauri/src/poster/mod.rs`
Expected: 定义（约 106）+ 唯一调用（约 78）恒传 `"media"`。

- [ ] **Step 2: 移除 table 参数，写死 media**

在 `src-tauri/src/poster/mod.rs`：把 `update_cover_path` 签名的 `table: &str` 参数删除，函数体内用到 `table` 的地方直接写 `"media"`（SQL 里表名写死 media）。更新唯一调用点去掉 `"media"` 实参。同步把该函数的 doc 注释改为不再提"可选 comic"（与 Task 8 Step 4 一致，此处最终定稿）。

- [ ] **Step 3: 编译 + 测试**

Run: `cd /Users/zhoumo/Documents/Claude/collector/src-tauri && cargo build 2>&1 | grep -iE "warning|error" || echo "clean"; cargo test poster 2>&1 | tail -5`
Expected: `clean` + poster 测试 PASS。

- [ ] **Step 4: 提交**

```bash
cd /Users/zhoumo/Documents/Claude/collector
git add src-tauri/src/poster/mod.rs
git commit -m "refactor(poster): update_cover_path 去掉恒为 media 的投机性 table 参数

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>"
```

---

### Task 15: 把 lib.rs 的启动迁移/seed 拆到 migrate.rs

**Files:**
- Create: `src-tauri/src/migrate.rs`
- Modify: `src-tauri/src/lib.rs`（移出 `migrate_subtitles_to_plain`/`migrate_covers_to_subdirs`/`release_seed_if_empty`，改为 `mod migrate;` 并调用 `migrate::*`）

- [ ] **Step 1: 读三个函数的完整实现与依赖**

Run:
```bash
cd /Users/zhoumo/Documents/Claude/collector
sed -n '77,169p' src-tauri/src/lib.rs
grep -n "migrate_subtitles_to_plain\|migrate_covers_to_subdirs\|release_seed_if_empty" src-tauri/src/lib.rs
```
Expected: 看到三个函数实现与它们在 `setup` 中的调用点、用到的 use（Db、path 等）。

- [ ] **Step 2: 新建 migrate.rs，迁移三个函数**

创建 `src-tauri/src/migrate.rs`，把三个函数原样搬入（改为 `pub(crate) fn`），补齐所需 `use`（如 `use crate::db::Db;`、`use tauri::Manager;`、`std::path::Path` 等，按函数体实际依赖）。保留原有中文注释与 `eprintln!` 迁移错误日志（那是有意的容错设计，非诊断残留，不删）。

- [ ] **Step 3: 在 lib.rs 声明模块并改调用**

在 `src-tauri/src/lib.rs`：
- 顶部模块声明区加 `mod migrate;`
- 删除三个函数的原定义。
- `setup` 中的调用改为 `migrate::migrate_subtitles_to_plain(&dir, &db)`、`migrate::migrate_covers_to_subdirs(&dir, &db)`、`migrate::release_seed_if_empty(app, &dir)`（按原调用签名）。
- 清理 lib.rs 中因函数移走而变得未使用的 `use`（cargo 会提示）。

- [ ] **Step 4: 编译（重点看 warning）**

Run: `cd /Users/zhoumo/Documents/Claude/collector/src-tauri && cargo build 2>&1 | grep -iE "warning|error" || echo "clean"`
Expected: `clean`（无 unused import、无未定义引用；若 migrate.rs 缺 use 会在此暴露，补齐）。

- [ ] **Step 5: 全量测试**

Run: `cd /Users/zhoumo/Documents/Claude/collector/src-tauri && cargo test 2>&1 | tail -5`
Expected: 全部 PASS（迁移逻辑若有测试随之通过）。

- [ ] **Step 6: 提交**

```bash
cd /Users/zhoumo/Documents/Claude/collector
git add src-tauri/src/lib.rs src-tauri/src/migrate.rs
git commit -m "refactor: 启动迁移/seed 释放拆到 migrate.rs，lib.rs 只留命令与装配

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>"
```

---

### Task 16: 把 fetch_posters 的内联闭包提为具名函数

**Files:**
- Modify: `src-tauri/src/lib.rs`（`fetch_posters` 内 `fetch_cover`/`suggest` 闭包提为函数）
- 可能 Modify: `src-tauri/src/poster/mod.rs`（若提到 poster 模块）

- [ ] **Step 1: 读 fetch_posters 全貌**

Run: `sed -n '528,654p' src-tauri/src/lib.rs`
Expected: 看到 `fetch_posters`（约 126 行）内嵌 `fetch_cover`（约 46 行）与 `suggest`（约 28 行）两个闭包，及它们捕获的变量（key、db、app 等）。

- [ ] **Step 2: 把两个闭包提为具名函数**

把 `fetch_cover`、`suggest` 提为普通函数（放在 `fetch_posters` 上方或 poster 模块内，取决于它们的依赖）。闭包捕获的变量改为显式函数参数（如 `key: &str`、`&db`、`&app` 等）。`fetch_posters` 内改为调用这两个函数。保持逻辑逐行等价，只是从闭包变具名函数 + 参数显式化。

- [ ] **Step 3: 编译**

Run: `cd /Users/zhoumo/Documents/Claude/collector/src-tauri && cargo build 2>&1 | grep -iE "warning|error" || echo "clean"`
Expected: `clean`。

- [ ] **Step 4: 全量测试**

Run: `cd /Users/zhoumo/Documents/Claude/collector/src-tauri && cargo test 2>&1 | tail -5`
Expected: 全部 PASS。

- [ ] **Step 5: 提交**

```bash
cd /Users/zhoumo/Documents/Claude/collector
git add src-tauri/src/lib.rs src-tauri/src/poster/mod.rs
git commit -m "refactor(poster): fetch_posters 内联闭包 fetch_cover/suggest 提为具名函数

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>"
```

---

## 批次三：前端三视图去重（tsc + 实际运行验证）

### Task 17: 提取三视图公共逻辑为共享工厂/组件

**Files:**
- Read: `src/views/VideoView.ts`、`src/views/ComicView.ts`、`src/views/GameView.ts`
- Create: `src/views/MediaLibraryView.ts`（公共工厂）或 `src/components/MediaLibraryToolbar.ts`（共享工具栏）
- Modify: 三个视图改为复用公共单元

- [ ] **Step 1: 读三视图，精确列出同构部分与差异**

Run:
```bash
cd /Users/zhoumo/Documents/Claude/collector
cat -n src/views/VideoView.ts
cat -n src/views/ComicView.ts
cat -n src/views/GameView.ts
```
Expected: 确认共同点（`type ViewMode = "folder" | "tree"`、`video-bar`/`view-toggle`/`vt-btn` 工具栏 HTML、`updateAddBtn()`、`mode/folderPath/treeSelected` 状态、`refresh()`、右键菜单"编辑/移动/删除"骨架）与差异（分类列表、API 调用如 `xxxDelete`、标题）。

- [ ] **Step 2: 设计公共单元接口**

创建 `src/views/MediaLibraryView.ts`，导出一个工厂函数，参数封装三视图的差异点：
```typescript
export interface MediaLibraryConfig {
  kind: "video" | "comic" | "game";
  cats?: string[];                       // 分类标签（video 有多分类；comic/game 视情况）
  onDelete: (id: number) => Promise<void>; // 对应 api.xxxDelete
  // 其余差异点按 Step 1 实际列出的补充（如列表加载函数、标题等）
}
export function MediaLibraryView(cfg: MediaLibraryConfig): HTMLElement {
  // 把三视图共有的工具栏 HTML、ViewMode 状态、updateAddBtn、refresh、
  // folder/tree 切换、右键菜单骨架统一实现；差异处调用 cfg 的字段。
}
```
接口字段以 Step 1 实际差异为准补全，不要遗漏任何一个视图用到的差异点。

- [ ] **Step 3: 用公共工厂重写 VideoView**

把 `src/views/VideoView.ts` 改为薄封装：构造 `MediaLibraryConfig`（kind: "video"、cats: 视频分类、onDelete: `api.videoDelete` 或现有删除 API）后 `return MediaLibraryView(cfg)`。保留 VideoView 特有的行为（若有）。

- [ ] **Step 4: 用公共工厂重写 ComicView 与 GameView**

同理把 `ComicView.ts`、`GameView.ts` 改为薄封装，各自传入 kind/cats/onDelete 等差异。三者的 `ViewMode`/工具栏/updateAddBtn/删除确认块统一由 `MediaLibraryView` 提供。

- [ ] **Step 5: 类型检查**

Run: `cd /Users/zhoumo/Documents/Claude/collector && npx tsc --noEmit`
Expected: 无输出（exit 0）。

- [ ] **Step 6: 构建**

Run: `cd /Users/zhoumo/Documents/Claude/collector && npm run build 2>&1 | tail -5`
Expected: 构建成功。

- [ ] **Step 7: 实际运行验证三库**

Run: `cd /Users/zhoumo/Documents/Claude/collector && npm run tauri:dev`（后台启动；若已在运行，HMR 会热更）。
手动验证（在应用窗口）：
- 视频库：工具栏显示、folder/tree 切换、分类切换、新增/编辑/移动/删除入口正常。
- 漫画库：同上。
- 游戏库：同上。
Expected: 三库行为与重构前一致，无控制台报错。

- [ ] **Step 8: 提交**

```bash
cd /Users/zhoumo/Documents/Claude/collector
git add src/views/MediaLibraryView.ts src/views/VideoView.ts src/views/ComicView.ts src/views/GameView.ts
git commit -m "refactor(views): 提取 MediaLibraryView 合并 video/comic/game 三视图同构逻辑

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>"
```

---

## 最终验收

### Task 18: 全量验证与收尾

- [ ] **Step 1: 后端全绿**

Run: `cd /Users/zhoumo/Documents/Claude/collector/src-tauri && cargo test 2>&1 | tail -5 && cargo build 2>&1 | grep -iE "warning|error" || echo "clean"`
Expected: 全部测试 PASS + `clean`。

- [ ] **Step 2: 前端全绿**

Run: `cd /Users/zhoumo/Documents/Claude/collector && npx tsc --noEmit && npm run build 2>&1 | tail -3`
Expected: 类型无错 + 构建成功。

- [ ] **Step 3: 运行抽查**

Run: `cd /Users/zhoumo/Documents/Claude/collector && npm run tauri:dev`
手动抽查：视频播放（含字幕）、漫画阅读、三库工具栏与增删改均正常。

- [ ] **Step 4: 工作树干净**

Run: `cd /Users/zhoumo/Documents/Claude/collector && git status --short && git log --oneline -14`
Expected: 工作树干净（无未提交改动，除本地 gitignore 临时物）；git log 显示批次一~三的各次提交。
