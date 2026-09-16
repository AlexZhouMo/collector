# 内置 ffmpeg/ffprobe 到 macOS + Windows 安装包 —— 实现计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 用 Tauri externalBin(sidecar) 机制把 ffmpeg/ffprobe 内置进 macOS 与 Windows 安装包，移除运行时对 brew 和安装时联网下载的依赖。

**Architecture:** 构建前由 Node 脚本按当前平台/架构下载完整静态 ffmpeg/ffprobe，重命名为 Tauri 要求的 `<name>-<target-triple>` 放入 `src-tauri/bin/`（不入 git）。`tauri.conf.json` 用 `bundle.externalBin` 声明，Tauri 打包时自动纳入并处理签名。运行时在 `setup` 阶段用 Tauri 路径 API 解析 sidecar 绝对路径存入 `OnceLock`，`transcode.rs` 从中读取后照旧用 `std::process::Command` 调用。

**Tech Stack:** Tauri 2.x、Rust（`std::process::Command` + `OnceLock`）、Node（下载脚本，`node:https`/`node:zlib`/内置解压）、NSIS。

参考 spec：`docs/superpowers/specs/2026-09-16-bundle-ffmpeg-sidecar-design.md`

---

## 文件结构

- **Create** `scripts/fetch-ffmpeg.mjs` —— 按平台下载/解压/重命名 ffmpeg、ffprobe 到 `src-tauri/bin/`，幂等，能校验 `-version`。
- **Create** `src-tauri/bin/.gitkeep` —— 保留目录（二进制本身被 gitignore）。
- **Modify** `.gitignore` —— 忽略 `src-tauri/bin/ffmpeg*`、`src-tauri/bin/ffprobe*`。
- **Modify** `package.json` —— 新增 `fetch:ffmpeg` 脚本；`beforeBuildCommand` 前置下载。
- **Modify** `src-tauri/tauri.conf.json` —— 加 `bundle.externalBin`；移除 nsis installerHooks。
- **Delete** `src-tauri/installer-hooks.nsi`。
- **Create** `src-tauri/src/player/ffmpeg_paths.rs` —— `OnceLock` 全局 + 解析 sidecar 路径的 `init`/`get` 接口。
- **Modify** `src-tauri/src/player/mod.rs` —— 声明 `pub mod ffmpeg_paths;`。
- **Modify** `src-tauri/src/lib.rs:663` setup 块 —— 调用 `ffmpeg_paths::init(app)`。
- **Modify** `src-tauri/src/player/transcode.rs` —— `ffmpeg_bin` 改读全局；简化 `ffmpeg_missing_hint`；更新单测。
- **Modify** `README.md` —— 打包说明改为自动获取、已内置。
- **Modify** `.github/workflows/build-windows.yml` —— build 前加 `npm run fetch:ffmpeg` 步骤。

---

## Task 1: 下载脚本 fetch-ffmpeg.mjs

**Files:**
- Create: `scripts/fetch-ffmpeg.mjs`
- Create: `src-tauri/bin/.gitkeep`

Tauri sidecar 命名规则：`externalBin` 里写 `bin/ffmpeg`，实际文件名必须为 `bin/ffmpeg-<target-triple>`（Windows 追加 `.exe`）。目标 triple：
- macOS：`aarch64-apple-darwin`、`x86_64-apple-darwin`
- Windows：`x86_64-pc-windows-msvc`、`aarch64-pc-windows-msvc`

下载源：
- macOS：`https://evermeet.cx/ffmpeg/getrelease/ffmpeg/zip` 与 `.../ffprobe/zip`（各含单个可执行文件，通用二进制，两个 darwin triple 可复用同一文件）。
- Windows x64：`https://github.com/BtbN/FFmpeg-Builds/releases/download/latest/ffmpeg-master-latest-win64-gpl.zip`；arm64：`...winarm64-gpl.zip`（zip 内 `bin/ffmpeg.exe`、`bin/ffprobe.exe`）。

- [ ] **Step 1: 写脚本骨架（参数解析 + target 映射）**

创建 `scripts/fetch-ffmpeg.mjs`：

```javascript
// 构建前下载完整静态 ffmpeg/ffprobe，按 Tauri sidecar 命名放入 src-tauri/bin/。
// 用法：node scripts/fetch-ffmpeg.mjs [--target <triple>] [--force]
// 无 --target 时按当前平台/架构推断。二进制不入 git（见 .gitignore）。
import { execFileSync } from 'node:child_process';
import { mkdirSync, existsSync, chmodSync, rmSync, renameSync, readdirSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { tmpdir } from 'node:os';

const __dirname = dirname(fileURLToPath(import.meta.url));
const BIN_DIR = join(__dirname, '..', 'src-tauri', 'bin');

const ARGS = process.argv.slice(2);
const FORCE = ARGS.includes('--force');
const targetArg = (() => {
  const i = ARGS.indexOf('--target');
  return i >= 0 ? ARGS[i + 1] : null;
})();

function currentTriple() {
  const p = process.platform, a = process.arch;
  if (p === 'darwin') return a === 'arm64' ? 'aarch64-apple-darwin' : 'x86_64-apple-darwin';
  if (p === 'win32') return a === 'arm64' ? 'aarch64-pc-windows-msvc' : 'x86_64-pc-windows-msvc';
  throw new Error(`不支持的平台: ${p}/${a}（本方案只处理 macOS + Windows）`);
}

const TARGET = targetArg || currentTriple();
const IS_WIN = TARGET.includes('windows');
const EXT = IS_WIN ? '.exe' : '';

console.log(`[fetch-ffmpeg] target = ${TARGET}`);
```

- [ ] **Step 2: 运行骨架验证 target 推断**

Run: `node scripts/fetch-ffmpeg.mjs`
Expected: 打印形如 `[fetch-ffmpeg] target = aarch64-apple-darwin`（在 Apple Silicon 上），无报错退出。

- [ ] **Step 3: 加下载/解压/放置逻辑**

在脚本末尾追加：

```javascript
function outPath(name) {
  return join(BIN_DIR, `${name}-${TARGET}${EXT}`);
}

// 已存在且能 -version 则跳过（除非 --force）
function isUsable(name) {
  const p = outPath(name);
  if (!existsSync(p)) return false;
  if (IS_WIN && TARGET !== currentTripleSafe()) return true; // 交叉目标无法本地执行，仅认存在
  try { execFileSync(p, ['-version'], { stdio: 'ignore' }); return true; }
  catch { return false; }
}
function currentTripleSafe() { try { return currentTriple(); } catch { return ''; } }

function download(url, dest) {
  console.log(`[fetch-ffmpeg] 下载 ${url}`);
  // 用 curl 兼容重定向与代理，避免手写 https 跟随 302
  execFileSync('curl', ['-fSL', '--retry', '3', '-o', dest, url], { stdio: 'inherit' });
}

function fetchMac() {
  mkdirSync(BIN_DIR, { recursive: true });
  for (const name of ['ffmpeg', 'ffprobe']) {
    if (!FORCE && isUsable(name)) { console.log(`[fetch-ffmpeg] 跳过 ${name}（已可用）`); continue; }
    const zip = join(tmpdir(), `${name}.zip`);
    download(`https://evermeet.cx/ffmpeg/getrelease/${name}/zip`, zip);
    const unzipDir = join(tmpdir(), `ff_${name}`);
    rmSync(unzipDir, { recursive: true, force: true });
    execFileSync('unzip', ['-o', zip, '-d', unzipDir], { stdio: 'inherit' });
    // evermeet zip 内就是单个可执行文件（名为 ffmpeg/ffprobe）
    const src = join(unzipDir, name);
    renameSync(src, outPath(name));
    chmodSync(outPath(name), 0o755);
    rmSync(zip, { force: true }); rmSync(unzipDir, { recursive: true, force: true });
    console.log(`[fetch-ffmpeg] 就绪 ${outPath(name)}`);
  }
}

function fetchWin() {
  mkdirSync(BIN_DIR, { recursive: true });
  const arch = TARGET.startsWith('aarch64') ? 'winarm64' : 'win64';
  if (!FORCE && isUsable('ffmpeg') && isUsable('ffprobe')) {
    console.log('[fetch-ffmpeg] 跳过 Windows（已存在）'); return;
  }
  const zip = join(tmpdir(), 'ffmpeg-win.zip');
  download(`https://github.com/BtbN/FFmpeg-Builds/releases/download/latest/ffmpeg-master-latest-${arch}-gpl.zip`, zip);
  const unzipDir = join(tmpdir(), 'ff_win');
  rmSync(unzipDir, { recursive: true, force: true });
  execFileSync('unzip', ['-o', zip, '-d', unzipDir], { stdio: 'inherit' });
  // 结构：<解压顶层目录>/bin/ffmpeg.exe、ffprobe.exe
  const top = readdirSync(unzipDir)[0];
  for (const name of ['ffmpeg', 'ffprobe']) {
    const src = join(unzipDir, top, 'bin', `${name}.exe`);
    renameSync(src, outPath(name));
    console.log(`[fetch-ffmpeg] 就绪 ${outPath(name)}`);
  }
  rmSync(zip, { force: true }); rmSync(unzipDir, { recursive: true, force: true });
}

if (IS_WIN) fetchWin(); else fetchMac();
console.log('[fetch-ffmpeg] 完成');
```

- [ ] **Step 4: 创建目录占位文件**

创建空文件 `src-tauri/bin/.gitkeep`（内容为空）。

- [ ] **Step 5: 实跑下载（当前平台 macOS）**

Run: `node scripts/fetch-ffmpeg.mjs`
Expected: 下载 ffmpeg、ffprobe 两个 zip 并解压，最终打印 `就绪 .../src-tauri/bin/ffmpeg-aarch64-apple-darwin` 与 `ffprobe-...`，末行 `[fetch-ffmpeg] 完成`。

- [ ] **Step 6: 验证产物可执行且幂等**

Run: `src-tauri/bin/ffmpeg-aarch64-apple-darwin -version | head -1 && node scripts/fetch-ffmpeg.mjs`
Expected: 首条打印 ffmpeg 版本行；第二次运行打印两行「跳过 …（已可用）」与「完成」，不再下载。

- [ ] **Step 7: 提交**

```bash
git add scripts/fetch-ffmpeg.mjs src-tauri/bin/.gitkeep
git commit -m "feat(build): 新增 fetch-ffmpeg 脚本，按平台下载 ffmpeg/ffprobe 到 sidecar 目录

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>"
```

---

## Task 2: .gitignore 与 package.json 接线

**Files:**
- Modify: `.gitignore`
- Modify: `package.json`

- [ ] **Step 1: 忽略下载的二进制**

在 `.gitignore` 的「Rust / Tauri build output」段落后追加：

```gitignore
# 构建前下载的 ffmpeg/ffprobe sidecar 二进制（不入库，用 scripts/fetch-ffmpeg.mjs 获取）
src-tauri/bin/ffmpeg*
src-tauri/bin/ffprobe*
```

- [ ] **Step 2: 验证二进制已被忽略**

Run: `git status --porcelain src-tauri/bin/`
Expected: 只可能显示 `.gitkeep`（若未提交），不显示任何 `ffmpeg*`/`ffprobe*` 文件。

- [ ] **Step 3: package.json 加脚本并前置到构建**

将 `package.json` 的 `scripts` 块改为（新增 `fetch:ffmpeg`，并让 `tauri:build` 前先下载）：

```json
  "scripts": {
    "dev": "vite",
    "build": "tsc && vite build",
    "preview": "vite preview",
    "tauri": "tauri",
    "tauri:dev": "tauri dev",
    "tauri:build": "npm run fetch:ffmpeg && tauri build",
    "fetch:ffmpeg": "node scripts/fetch-ffmpeg.mjs"
  },
```

（注：`tauri.conf.json` 的 `beforeBuildCommand` 是 `npm run build`，负责前端；ffmpeg 下载放在 `tauri:build` npm 脚本层，避免污染 `tauri dev`。CI 单独显式调用，见 Task 6。）

- [ ] **Step 4: 验证 fetch:ffmpeg 可用**

Run: `npm run fetch:ffmpeg`
Expected: 走 Task 1 的幂等分支，打印「跳过 … 完成」。

- [ ] **Step 5: 提交**

```bash
git add .gitignore package.json
git commit -m "chore(build): 忽略 sidecar 二进制，package.json 接入 fetch:ffmpeg

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>"
```

---

## Task 3: Tauri 配置改用 externalBin，移除 NSIS 下载钩子

**Files:**
- Modify: `src-tauri/tauri.conf.json`
- Delete: `src-tauri/installer-hooks.nsi`

- [ ] **Step 1: 加 externalBin，移除 nsis installerHooks**

将 `src-tauri/tauri.conf.json` 的 `bundle` 块改为：

```json
  "bundle": {
    "active": true,
    "targets": "all",
    "externalBin": [
      "bin/ffmpeg",
      "bin/ffprobe"
    ],
    "icon": [
      "icons/32x32.png",
      "icons/128x128.png",
      "icons/128x128@2x.png",
      "icons/icon.icns",
      "icons/icon.ico"
    ],
    "resources": {
      "resources/seed": "seed"
    }
  }
```

（删除了整个 `"windows": { "nsis": { "installerHooks": ... } }` 子块。）

- [ ] **Step 2: 删除 NSIS 钩子文件**

```bash
git rm src-tauri/installer-hooks.nsi
```

- [ ] **Step 3: 校验配置 JSON 合法**

Run: `node -e "JSON.parse(require('fs').readFileSync('src-tauri/tauri.conf.json','utf8')); console.log('ok')"`
Expected: 打印 `ok`。

- [ ] **Step 4: 提交**

```bash
git add src-tauri/tauri.conf.json
git commit -m "build(tauri): externalBin 打包 ffmpeg/ffprobe，移除 Windows 联网下载钩子

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>"
```

---

## Task 4: Rust 端 sidecar 路径解析（ffmpeg_paths 模块）

**Files:**
- Create: `src-tauri/src/player/ffmpeg_paths.rs`
- Modify: `src-tauri/src/player/mod.rs:4`（在 `pub mod transcode;` 附近加声明）
- Modify: `src-tauri/src/lib.rs:663`（setup 块内调用 init）

Tauri 在打包后把 sidecar 放到可执行文件旁，文件名为 `ffmpeg-<triple>`（Win 带 `.exe`）。`app.path().resolve` 对 sidecar 不直接适用，最稳妥是取「当前可执行文件所在目录」拼接 sidecar 文件名；开发态（`tauri dev`/`cargo test`）无 sidecar 时返回 `None`，由 transcode 回退裸命令名。

- [ ] **Step 1: 写模块（含单测：未初始化时 get 返回 None）**

创建 `src-tauri/src/player/ffmpeg_paths.rs`：

```rust
//! 解析并缓存内置 ffmpeg/ffprobe(sidecar) 的绝对路径。
//! 打包后 sidecar 位于可执行文件旁，命名为 `ffmpeg-<target-triple>`(Win 带 .exe)。
//! 开发态/测试无 sidecar 时保持未初始化，transcode 回退裸命令名走 PATH。
use std::path::PathBuf;
use std::sync::OnceLock;

/// 解析后的两个二进制绝对路径。init 成功后填充。
struct FfmpegPaths {
    ffmpeg: PathBuf,
    ffprobe: PathBuf,
}

static PATHS: OnceLock<FfmpegPaths> = OnceLock::new();

/// 返回内置 ffmpeg 的绝对路径（已初始化且文件存在时）。
pub fn ffmpeg() -> Option<PathBuf> {
    PATHS.get().map(|p| p.ffmpeg.clone())
}

/// 返回内置 ffprobe 的绝对路径（已初始化且文件存在时）。
pub fn ffprobe() -> Option<PathBuf> {
    PATHS.get().map(|p| p.ffprobe.clone())
}

/// sidecar 文件名：`<name>-<triple>`，Windows 追加 .exe。
/// triple 由构建期 env `TARGET`（tauri-build 注入 `TAURI_ENV_TARGET_TRIPLE`）决定。
fn sidecar_name(name: &str) -> String {
    let triple = option_env!("TAURI_ENV_TARGET_TRIPLE").unwrap_or("");
    let base = if triple.is_empty() { name.to_string() } else { format!("{name}-{triple}") };
    if cfg!(windows) { format!("{base}.exe") } else { base }
}

/// 在应用启动时调用：解析可执行文件旁的 sidecar 路径，存在则缓存。
pub fn init() {
    let Ok(exe) = std::env::current_exe() else { return };
    let Some(dir) = exe.parent() else { return };
    let ffmpeg = dir.join(sidecar_name("ffmpeg"));
    let ffprobe = dir.join(sidecar_name("ffprobe"));
    if ffmpeg.exists() && ffprobe.exists() {
        let _ = PATHS.set(FfmpegPaths { ffmpeg, ffprobe });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn unset_returns_none() {
        // 测试进程未 init（或 sidecar 不存在），两个 getter 均为 None
        assert!(ffmpeg().is_none());
        assert!(ffprobe().is_none());
    }
    #[test]
    fn sidecar_name_has_triple_or_bare() {
        let n = sidecar_name("ffmpeg");
        // 至少包含基名；Windows 带 .exe
        assert!(n.contains("ffmpeg"));
        #[cfg(windows)]
        assert!(n.ends_with(".exe"));
    }
}
```

- [ ] **Step 2: 声明模块**

在 `src-tauri/src/player/mod.rs` 的 `pub mod transcode;`（第 4 行）下一行加入：

```rust
pub mod ffmpeg_paths;
```

- [ ] **Step 3: setup 里调用 init**

在 `src-tauri/src/lib.rs` 的 `.setup(|app| {` 块内、`let dir = ...` 之前加入一行：

```rust
            player::ffmpeg_paths::init();
```

- [ ] **Step 4: 编译并跑新模块单测**

Run: `cd src-tauri && cargo test ffmpeg_paths 2>&1 | tail -20`
Expected: 编译通过；`unset_returns_none`、`sidecar_name_has_triple_or_bare` 均 PASS。

- [ ] **Step 5: 提交**

```bash
git add src-tauri/src/player/ffmpeg_paths.rs src-tauri/src/player/mod.rs src-tauri/src/lib.rs
git commit -m "feat(player): 启动时解析内置 ffmpeg/ffprobe sidecar 路径并缓存

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>"
```

---

## Task 5: transcode.rs 改用内置路径，简化缺失提示

**Files:**
- Modify: `src-tauri/src/player/transcode.rs:13-26`（`ffmpeg_bin`）
- Modify: `src-tauri/src/player/transcode.rs:44-58`（`ffmpeg_missing_hint`）
- Modify: `src-tauri/src/player/transcode.rs:180-187`（单测 `ffmpeg_hint_contains_guidance`）

- [ ] **Step 1: 改写 ffmpeg_bin 读全局路径**

将 `src-tauri/src/player/transcode.rs` 的 `ffmpeg_bin` 函数（第 13–26 行，含上方两行注释）整体替换为：

```rust
/// 定位 ffmpeg/ffprobe：优先用启动时解析的内置 sidecar 绝对路径；
/// 开发态/未初始化时回退裸命令名走 PATH（便于 tauri dev / cargo test）。
fn ffmpeg_bin(name: &str) -> PathBuf {
    let resolved = match name {
        "ffmpeg" => crate::player::ffmpeg_paths::ffmpeg(),
        "ffprobe" => crate::player::ffmpeg_paths::ffprobe(),
        _ => None,
    };
    resolved.unwrap_or_else(|| PathBuf::from(name))
}
```

- [ ] **Step 2: 简化缺失提示文案**

将 `ffmpeg_missing_hint` 函数（第 44–58 行，含上方注释）整体替换为：

```rust
/// 内置 ffmpeg/ffprobe 缺失或损坏时的中文提示（引导重装）。
fn ffmpeg_missing_hint(tool: &str) -> String {
    format!(
        "视频播放需要内置的 {tool} 组件，但未找到或无法运行。\n\n\
         Collector 已随安装包内置 ffmpeg/ffprobe，此提示通常意味着安装文件损坏或被安全软件拦截。\n\n\
         【解决办法】请重新下载并安装 Collector；若仍失败，可将应用加入安全软件白名单后重试。"
    )
}
```

- [ ] **Step 3: 更新对应单测断言**

将单测 `ffmpeg_hint_contains_guidance`（第 180–187 行）整体替换为：

```rust
    #[test]
    fn ffmpeg_hint_mentions_reinstall() {
        let h = ffmpeg_missing_hint("ffmpeg");
        assert!(h.contains("内置"));
        assert!(h.contains("重新下载并安装 Collector"));
        assert!(h.contains("ffmpeg"));
    }
```

- [ ] **Step 4: 编译并跑 transcode 单测**

Run: `cd src-tauri && cargo test transcode 2>&1 | tail -25`
Expected: 编译通过；`cached_path_is_stable_and_mp4`、`lru_deletes_oldest_over_limit`、`ffmpeg_hint_mentions_reinstall` 均 PASS。

- [ ] **Step 5: 跑全量后端测试确保无回归**

Run: `cd src-tauri && cargo test 2>&1 | tail -15`
Expected: 全部 PASS，无编译错误/警告失败。

- [ ] **Step 6: 提交**

```bash
git add src-tauri/src/player/transcode.rs
git commit -m "refactor(player): transcode 改用内置 sidecar 路径，缺失提示改为引导重装

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>"
```

---

## Task 6: CI 与 README 更新

**Files:**
- Modify: `.github/workflows/build-windows.yml`
- Modify: `README.md:26-33`（开发依赖段）与相关用户说明

- [ ] **Step 1: CI 在构建前下载 ffmpeg**

在 `.github/workflows/build-windows.yml` 的 `- name: Install frontend deps` 步骤之后、`- name: Build Tauri (NSIS)` 之前，插入：

```yaml
      - name: Fetch ffmpeg sidecar
        run: node scripts/fetch-ffmpeg.mjs --target x86_64-pc-windows-msvc
```

（Windows runner 上 `unzip`/`curl` 可用；`--target` 显式指定以匹配 NSIS x64 构建。）

- [ ] **Step 2: 更新 README 开发依赖段**

将 `README.md` 的「开发依赖」中 ffmpeg 条目（约第 30–33 行）替换为：

```markdown
- **ffmpeg**（视频转码/探测；提供 `ffmpeg` 与 `ffprobe`）
  - 已内置：macOS `.app` 与 Windows 安装包均随包自带 ffmpeg/ffprobe，终端用户**无需自行安装**。
  - 开发/打包：构建前运行 `npm run fetch:ffmpeg` 自动下载对应平台二进制到 `src-tauri/bin/`（不入 git）；`npm run tauri:build` 会自动前置执行。
```

- [ ] **Step 3: 校验 workflow YAML 合法**

Run: `node -e "const y=require('fs').readFileSync('.github/workflows/build-windows.yml','utf8'); if(!/Fetch ffmpeg sidecar/.test(y)) throw new Error('missing step'); console.log('ok')"`
Expected: 打印 `ok`。

- [ ] **Step 4: 提交**

```bash
git add .github/workflows/build-windows.yml README.md
git commit -m "docs+ci: README 说明 ffmpeg 已内置，CI 构建前下载 sidecar

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>"
```

---

## Task 7: 端到端验证（本机 macOS 打包）

**Files:** 无（验证性任务）

- [ ] **Step 1: 完整打包**

Run: `npm run tauri:build 2>&1 | tail -30`
Expected: 先执行 `fetch:ffmpeg`（已缓存则跳过），再 `tauri build`；末尾产出 `.app`/`.dmg`，无错误。

- [ ] **Step 2: 确认 sidecar 已进包**

Run: `ls -la "src-tauri/target/release/bundle/macos/Collector.app/Contents/MacOS/"`
Expected: 目录下同时存在主程序 `Collector` 与 `ffmpeg-aarch64-apple-darwin`、`ffprobe-aarch64-apple-darwin`（或对应 x86_64）。

- [ ] **Step 3: 无系统 ffmpeg 下播放验证（手动）**

临时让 PATH 中 ffmpeg 不可见后启动打包应用，打开一个 H.264 MKV 视频。
Expected: 视频能正常 remux 并播放，说明用的是内置 sidecar 而非系统 ffmpeg。

（若本机装了 brew ffmpeg，可临时 `PATH= open Collector.app` 或重命名 brew 的 ffmpeg 验证；确认后恢复。）

- [ ] **Step 4: 记录验证结果**

在提交信息或 PR 描述中记录：打包产物含 sidecar、无系统 ffmpeg 时播放正常。无需额外提交（除非验证中发现并修了问题）。

---

## Self-Review 记录

- **Spec 覆盖**：externalBin 打包(Task 3)、构建前下载脚本(Task 1)、不入 git(Task 2)、OnceLock 缓存路径(Task 4)、transcode 改造+提示简化+单测(Task 5)、README+CI(Task 6)、去除 NSIS 下载钩子(Task 3)、macOS 去 brew(整体) —— 均有对应任务；Linux/精简构建/shell 插件属非目标，未纳入 ✔
- **占位扫描**：无 TBD/TODO；每个改代码的 Step 均给出完整代码或完整命令 ✔
- **类型/命名一致**：`ffmpeg_paths::ffmpeg()/ffprobe()/init()` 在 Task 4 定义、Task 5 使用一致；sidecar 命名 `<name>-<triple>` 在脚本(Task 1)与 Rust(Task 4)一致 ✔
