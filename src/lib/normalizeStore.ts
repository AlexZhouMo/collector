// 工具箱三个长任务（下载字幕 / 字幕校准 / 漫画归档）的模块级单例状态。
// 不随视图销毁：切菜单后任务继续、进度与结果留存，切回工具箱可续显。
// 后端命令在独立线程跑完并全局 emit 进度；此 store 持有任务 Promise 与最新进度/结果。
import { api } from "./ipc";
import type { SubReport, ArchiveReport, FillReport } from "./ipc";
import { esc } from "./escape";
import { nameCollator } from "./displayPath";
import { icon } from "./icons";

export type TaskKey = "subtitle" | "comic" | "subfill";
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
  subfill: initState(),
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
  } else if (key === "subfill") {
    const { done, total, current } = payload as { done: number; total: number; current: string };
    s.pct = total ? Math.round((done / total) * 100) : 0;
    s.statusText = total
      ? `下载字幕 ${done}/${total}${current ? "：" + current : ""}`
      : "正在扫描缺失字幕…";
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
  if (kind.includes("交叉") || kind.includes("乱码")) return "#ff9b9b";
  if (kind.includes("未合并") || kind.includes("多于")) return "#ffb07a";
  if (kind.includes("对话")) return "#9db8ff";
  if (kind.includes("道具") || kind.includes("外语") || kind.includes("歌曲") || kind.includes("音乐")) return "#c3a8ff";
  if (kind.includes("漏译")) return "#ffd479";
  if (kind.includes("字体")) return "#7fd6e0";
  if (kind.includes("存疑")) return "#f0a6ca";
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

/** 告警类型 → 行尾按钮。两种：
 *  - copy：把字符串复制到剪贴板，不改原文，供人工粘贴修改；
 *  - action：直接改原文并重校（目前只有 tag-foreign：行首插入「[外语] 」），需能定位原文行。 */
type RowAction = { icon: Parameters<typeof icon>[0]; title: string } & ({ copy: string } | { action: "tag-foreign" });
const ROW_ACTIONS: Record<string, RowAction> = {
  "疑似漏译": { action: "tag-foreign", icon: "langTag", title: "在句首插入[外语]标签" },
  "非标准音乐符": { copy: "♪", icon: "songMark", title: "复制「♪」到剪贴板" },
  "疑似未合并中英": { copy: "\\N{\\fnArial\\fs24\\c&H50DCFF&}", icon: "mergeLines", title: "复制中英分隔符 \\N{\\fnArial\\fs24\\c&H50DCFF&} 到剪贴板" },
};

function issueRowHtml(file: string, i: SubReport["issues"][number]): string {
  const editable = i.src_lines && i.src_lines.length > 0;
  const srcAttr = editable ? i.src_lines.join(",") : "";
  const hint = editable ? ' title="双击编辑"' : ' title="该文件无法解码，不能编辑"';
  const act = ROW_ACTIONS[i.kind];
  let copyBtn = "";
  if (act && "copy" in act) {
    copyBtn = `<button class="sub-copy-btn" data-copy="${esc(act.copy)}" title="${esc(act.title)}">${icon(act.icon, 14)}</button>`;
  } else if (act && editable) {
    copyBtn = `<button class="sub-copy-btn" data-action="${act.action}" title="${esc(act.title)}">${icon(act.icon, 14)}</button>`;
  }
  return `<div class="sub-issue${editable ? " editable" : ""}" data-file="${esc(file)}" data-src-lines="${srcAttr}" data-kind="${esc(i.kind)}"${hint}><span class="sub-kind" style="--k:${kindColor(i.kind)}">${esc(i.kind)}</span>${i.start ? `<span class="sub-loc">${esc(i.start)} → ${esc(i.end)}</span>` : ""}<span class="sub-text">${esc(i.text)}</span>${copyBtn}</div>`;
}

/** 某个文件的提示行 HTML（供保存后局部刷新）。 */
export function renderIssueRows(file: string, issues: SubReport["issues"]): string {
  return issues.map((i) => issueRowHtml(file, i)).join("");
}

// 三种结果页同一布局：右上角汇总徽标 + 按组折叠的明细行
const summaryHtml = (badge: string): string => `<div class="sub-summary">${badge}</div>`;
const plainRow = (kind: string, color: string, text: string): string =>
  `<div class="sub-issue"><span class="sub-kind" style="--k:${color}">${esc(kind)}</span><span class="sub-text">${esc(text)}</span></div>`;
const groupHtml = (title: string, count: number, body: string, open = false, attrs = ""): string =>
  `<details class="sub-file"${attrs}${open ? " open" : ""}><summary><span class="sub-fname">${esc(title)}</span><span class="sub-badge">${count}</span></summary><div class="sub-issues">${body}</div></details>`;

function renderSubReport(reports: SubReport[]): string {
  const totalIssues = reports.reduce((a, r) => a + r.issues.length, 0);
  let html = summaryHtml(totalIssues
    ? `<span class="sub-count-badge has">${totalIssues} 条提示</span>`
    : `<span class="sub-count-badge none">无质检问题</span>`);
  reports.filter(r => r.issues.length).forEach(r => {
    html += groupHtml(r.file, r.issues.length, renderIssueRows(r.file, r.issues), false, ` data-file="${esc(r.file)}"`);
  });
  return html;
}

function renderComicReport(reports: ArchiveReport[]): string {
  const ok = reports.filter(r => r.status === "成功").length;
  const skip = reports.filter(r => r.status.includes("跳过")).length;
  const fail = reports.filter(r => r.status.startsWith("失败")).length;
  let html = summaryHtml(`<span>✓ 处理 ${reports.length} 卷</span><span class="sub-count-badge ${fail ? "has" : "none"}">成功 ${ok}・跳过 ${skip}・失败 ${fail}</span>`);
  const byManga = new Map<string, ArchiveReport[]>();
  reports.forEach(r => { const a = byManga.get(r.manga) ?? []; a.push(r); byManga.set(r.manga, a); });
  byManga.forEach((vols, manga) => {
    const rows = vols.map(v => {
      const color = v.status === "成功" ? "#8fdca0" : v.status.includes("跳过") ? "#9db8ff" : "#ff9b9b";
      return plainRow(v.vol, color, v.status === "成功" ? `${v.pages} 页` : v.status);
    }).join("");
    html += groupHtml(manga, vols.length, rows, true);
  });
  return html;
}

function renderFillReport(r: FillReport): string {
  // 汇总徽标固定用绿色（校准结果是黄色）
  let html = summaryHtml(`<span class="sub-count-badge none">下载 ${r.downloaded.length}・未找到 ${r.not_found.length}・失败 ${r.failed.length}</span>`);
  if (r.aborted) html += plainRow("已中止", "#ff9b9b", r.aborted);
  const group = (title: string, color: string, lines: string[], open = false): string =>
    lines.length ? groupHtml(title, lines.length, lines.map((t) => plainRow(title, color, t)).join(""), open) : "";
  html += group("已下载", "#8fdca0", r.downloaded);
  html += group("未找到", "#9db8ff", r.not_found);
  html += group("失败", "#ff9b9b", r.failed.map((f) => `${f.file}　${f.reason}`), true);
  return html;
}

// ---- 任务启动（Promise 由 store 持有，与视图无关）----

// 下载字幕与字幕校准互为对手：都读写字幕输入目录，不能同时跑；
// 一方开始时清掉另一方的进度和结果页，页面上只留正在做的这一个。
const RIVAL: Partial<Record<TaskKey, TaskKey>> = { subtitle: "subfill", subfill: "subtitle" };

function begin(key: TaskKey): boolean {
  const rival = RIVAL[key];
  if (states[key].status === "running" || (rival && states[rival].status === "running")) return false;
  if (rival) {
    states[rival] = initState();
    if (rival === "subtitle") lastSubReports = [];
  }
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

export async function startSubtitleFill(dir: string): Promise<void> {
  if (!begin("subfill")) return;
  const s = states.subfill;
  try {
    const r = await api.fillSubtitles(dir);
    const missing = r.total - r.existing;
    s.pct = 100;
    s.statusText = r.aborted
      ? `已中止：下载 ${r.downloaded.length} 个`
      : missing ? `完成：缺失 ${missing} 个，下载 ${r.downloaded.length} 个` : "所有视频都已有字幕";
    s.resultHtml = missing ? renderFillReport(r) : "";
    s.status = r.aborted ? "error" : "done";
  } catch (err) {
    s.statusText = "下载字幕失败：" + String(err);
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
  if (idx < 0) return; // 结果页已被「下载字幕」清掉
  lastSubReports[idx] = { file, issues };
  states.subtitle.resultHtml = renderSubReport(lastSubReports);
}
