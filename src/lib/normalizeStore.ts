// 工具箱两个长任务（字幕/漫画归档）的模块级单例状态。
// 不随视图销毁：切菜单后任务继续、进度与结果留存，切回工具箱可续显。
// 后端命令在独立线程跑完并全局 emit 进度；此 store 持有任务 Promise 与最新进度/结果。
import { api } from "./ipc";
import type { SubReport, ArchiveReport } from "./ipc";
import { esc } from "./escape";
import { nameCollator } from "./displayPath";
import { icon } from "./icons";

export type TaskKey = "subtitle" | "comic";
type TaskStatus = "idle" | "running" | "done" | "error";

interface TaskState {
  status: TaskStatus;
  pct: number;          // 进度条百分比 0-100
  statusText: string;   // 进度条下方文字
  resultHtml: string;   // 结果区 HTML（完成后填充）
}

function initState(): TaskState {
  return { status: "idle", pct: 0, statusText: "", resultHtml: "" };
}

const states: Record<TaskKey, TaskState> = {
  subtitle: initState(),
  comic: initState(),
};

const subs = new Set<() => void>();
function notify() { subs.forEach((f) => f()); }

export function subscribe(fn: () => void): () => void {
  subs.add(fn);
  return () => subs.delete(fn);
}

export function getState(key: TaskKey): TaskState {
  return states[key];
}

/** 进度事件入口：由 main.ts 注册的全局监听回调调用。 */
export function applyProgress(key: TaskKey, payload: unknown): void {
  const s = states[key];
  if (s.status !== "running") return; // 非运行态忽略（收尾帧等）
  if (key === "subtitle") {
    const { done, total } = payload as { done: number; total: number };
    s.pct = total ? Math.round((done / total) * 100) : 0;
    s.statusText = `已处理 ${done}/${total}`;
  } else {
    const { manga, vol, done_images, total_images } = payload as
      { manga: string; vol: string; done_images: number; total_images: number };
    s.pct = total_images ? Math.round((done_images / total_images) * 100) : 0;
    s.statusText = manga ? `正在：${manga} ${vol}｜图片 ${done_images}/${total_images}` : "准备中…";
  }
  notify();
}

// ---- 结果渲染（从 NormalizeView 迁入，逻辑不变）----

export const kindColor = (kind: string): string => {
  if (kind.includes("交叉")) return "#ff9b9b";
  if (kind.includes("未合并") || kind.includes("多于")) return "#ffb07a";
  if (kind.includes("对话")) return "#9db8ff";
  if (kind.includes("道具") || kind.includes("外语") || kind.includes("歌曲") || kind.includes("音乐")) return "#c3a8ff";
  if (kind.includes("漏译")) return "#ffd479";
  return "#ffb08a";
};

// 最近一次字幕报告的结构化数据（供单文件重校刷新）。
let lastSubReports: SubReport[] = [];

// 结果排序：电影 > 动漫 > 剧集 > 其它；类型内按路径逐段字母序（英文在前、中文按拼音、数字按数值）。
const TYPE_ORDER = ["电影", "动漫", "剧集"];
const splitPath = (p: string): string[] => p.split(/[\\/]+/).filter(Boolean);
const typeRank = (segs: string[]): number => {
  for (const s of segs) {
    const i = TYPE_ORDER.indexOf(s);
    if (i >= 0) return i;
  }
  return TYPE_ORDER.length;
};
function sortSubReports(reports: SubReport[]): SubReport[] {
  const keyed = reports.map((r) => { const segs = splitPath(r.file); return { r, segs, rank: typeRank(segs) }; });
  keyed.sort((a, b) => {
    if (a.rank !== b.rank) return a.rank - b.rank;
    const n = Math.min(a.segs.length, b.segs.length);
    for (let i = 0; i < n; i++) {
      const c = nameCollator.compare(a.segs[i], b.segs[i]);
      if (c) return c;
    }
    return a.segs.length - b.segs.length;
  });
  return keyed.map((k) => k.r);
}

/** 告警类型 → 行尾「复制到剪贴板」按钮：复制的字符串、图标、提示文字。不改原文，供人工粘贴修改。 */
const COPY_ACTIONS: Record<string, { text: string; icon: Parameters<typeof icon>[0]; title: string }> = {
  "疑似漏译": { text: "[外语] ", icon: "langTag", title: "复制「[外语] 」标记到剪贴板" },
  "非标准音乐符": { text: "∮", icon: "songMark", title: "复制「∮」到剪贴板" },
  "疑似未合并中英": { text: "\\N{\\fnArial\\fs30}", icon: "mergeLines", title: "复制中英分隔符 \\N{\\fnArial\\fs30} 到剪贴板" },
};

function issueRowHtml(file: string, i: SubReport["issues"][number]): string {
  const editable = i.src_lines && i.src_lines.length > 0;
  const srcAttr = editable ? i.src_lines.join(",") : "";
  const hint = editable ? ' title="双击编辑"' : ' title="该文件无法解码，不能编辑"';
  const act = COPY_ACTIONS[i.kind];
  const copyBtn = act
    ? `<button class="sub-copy-btn" data-copy="${esc(act.text)}" title="${esc(act.title)}">${icon(act.icon, 14)}</button>` : "";
  return `<div class="sub-issue${editable ? " editable" : ""}" data-file="${esc(file)}" data-src-lines="${srcAttr}" data-kind="${esc(i.kind)}"${hint}><span class="sub-kind" style="--k:${kindColor(i.kind)}">${esc(i.kind)}</span>${i.start ? `<span class="sub-loc">${esc(i.start)} → ${esc(i.end)}</span>` : ""}<span class="sub-text">${esc(i.text)}</span>${copyBtn}</div>`;
}

/** 某个文件的提示行 HTML（供保存后局部刷新）。 */
export function renderIssueRows(file: string, issues: SubReport["issues"]): string {
  return issues.map((i) => issueRowHtml(file, i)).join("");
}

function renderSubReport(reports: SubReport[]): string {
  const totalIssues = reports.reduce((a, r) => a + r.issues.length, 0);
  const countBadge = totalIssues
    ? `<span class="sub-count-badge has">${totalIssues} 条提示</span>`
    : `<span class="sub-count-badge none">无质检问题</span>`;
  let html = `<div class="sub-summary"><span>✓ 处理 ${reports.length} 个文件</span>${countBadge}</div>`;
  reports.filter(r => r.issues.length).forEach(r => {
    html += `<details class="sub-file" data-file="${esc(r.file)}"><summary><span class="sub-fname">${esc(r.file)}</span><span class="sub-badge">${r.issues.length}</span></summary><div class="sub-issues">${renderIssueRows(r.file, r.issues)}</div></details>`;
  });
  return html;
}

function renderComicReport(reports: ArchiveReport[]): string {
  const ok = reports.filter(r => r.status === "成功").length;
  const skip = reports.filter(r => r.status.includes("跳过")).length;
  const fail = reports.filter(r => r.status.startsWith("失败")).length;
  const badge = `<span class="sub-count-badge ${fail ? "has" : "none"}">成功 ${ok}・跳过 ${skip}・失败 ${fail}</span>`;
  let html = `<div class="sub-summary"><span>✓ 处理 ${reports.length} 卷</span>${badge}</div>`;
  const byManga = new Map<string, ArchiveReport[]>();
  reports.forEach(r => { const a = byManga.get(r.manga) ?? []; a.push(r); byManga.set(r.manga, a); });
  byManga.forEach((vols, manga) => {
    const rows = vols.map(v => {
      const color = v.status === "成功" ? "#8fdca0" : v.status.includes("跳过") ? "#9db8ff" : "#ff9b9b";
      const extra = v.status === "成功" ? `${v.pages} 页` : v.status;
      return `<div class="sub-issue"><span class="sub-kind" style="--k:${color}">${esc(v.vol)}</span><span class="sub-text">${esc(extra)}</span></div>`;
    }).join("");
    html += `<details class="sub-file" open><summary><span class="sub-fname">${esc(manga)}</span><span class="sub-badge">${vols.length}</span></summary><div class="sub-issues">${rows}</div></details>`;
  });
  return html;
}

// ---- 任务启动（Promise 由 store 持有，与视图无关）----

function begin(key: TaskKey): boolean {
  if (states[key].status === "running") return false; // 防重入
  states[key] = { status: "running", pct: 0, statusText: "准备中…", resultHtml: "" };
  notify();
  return true;
}

export async function startSubtitle(dir: string): Promise<void> {
  if (!begin("subtitle")) return;
  const s = states.subtitle;
  try {
    const reports = await api.normalizeSubtitles(dir);
    lastSubReports = sortSubReports(reports);
    s.pct = 100;
    s.statusText = `完成：处理 ${reports.length} 个文件`;
    s.resultHtml = renderSubReport(lastSubReports);
    s.status = "done";
  } catch (err) {
    s.statusText = "字幕批量校准失败：" + String(err);
    s.status = "error";
  }
  notify();
}

export async function startComic(): Promise<void> {
  if (!begin("comic")) return;
  const s = states.comic;
  try {
    const reports = await api.archiveComics();
    s.pct = 100;
    s.statusText = `完成：处理 ${reports.length} 卷`;
    s.resultHtml = renderComicReport(reports);
    s.status = "done";
  } catch (err) {
    s.statusText = "漫画自动归档失败：" + String(err);
    s.status = "error";
  }
  notify();
}

/** 保存编辑后：用重校得到的新 issues 替换指定 file 那一组并更新缓存的结果 HTML。
 *  不 notify：当前页面不重绘（保留展开状态与滚动位置），下次进入工具箱时才显示新结果。 */
export function updateFileIssues(file: string, issues: SubReport["issues"]): void {
  const idx = lastSubReports.findIndex((r) => r.file === file);
  if (idx >= 0) lastSubReports[idx] = { file, issues };
  states.subtitle.resultHtml = renderSubReport(lastSubReports);
}
