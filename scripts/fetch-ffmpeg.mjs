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

// 在解压目录里递归找到指定可执行文件（应对 zip 内层级结构差异）
function findExecutable(root, baseName) {
  const want = IS_WIN ? `${baseName}.exe` : baseName;
  const stack = [root];
  while (stack.length) {
    const dir = stack.pop();
    for (const entry of readdirSync(dir, { withFileTypes: true })) {
      const full = join(dir, entry.name);
      if (entry.isDirectory()) stack.push(full);
      else if (entry.name === want) return full;
    }
  }
  return null;
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
    // evermeet zip 内就是单个可执行文件（名为 ffmpeg/ffprobe），容错递归查找
    const src = findExecutable(unzipDir, name);
    if (!src) throw new Error(`解压后未找到 ${name}（${unzipDir}）`);
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
  // 结构：<解压顶层目录>/bin/ffmpeg.exe、ffprobe.exe，递归查找以容错层级差异
  for (const name of ['ffmpeg', 'ffprobe']) {
    const src = findExecutable(unzipDir, name);
    if (!src) throw new Error(`解压后未找到 ${name}.exe（${unzipDir}）`);
    renameSync(src, outPath(name));
    console.log(`[fetch-ffmpeg] 就绪 ${outPath(name)}`);
  }
  rmSync(zip, { force: true }); rmSync(unzipDir, { recursive: true, force: true });
}

if (IS_WIN) fetchWin(); else fetchMac();
console.log('[fetch-ffmpeg] 完成');
