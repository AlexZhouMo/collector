// 工具箱三个长任务（海报/字幕/漫画归档）的模块级单例状态。
// 不随视图销毁：切菜单后任务继续、进度与结果留存，切回工具箱可续显。
// 后端命令在独立线程跑完并全局 emit 进度；此 store 持有任务 Promise 与最新进度/结果。
import { api } from "./ipc";
import type { SubReport, FailedItem, ArchiveReport } from "./ipc";
import { esc } from "./escape";

export type TaskKey = "poster" | "subtitle" | "comic";
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
  poster: initState(),
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
  if (key === "poster") {
    const { done, total } = payload as { done: number; total: number; current_title?: string };
    s.pct = total ? Math.round((done / total) * 100) : 0;
    s.statusText = `抓取中… ${done}/${total}`;
  } else if (key === "subtitle") {
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
  if (kind.includes("道具") || kind.includes("外语") || kind.includes("歌曲")) return "#c3a8ff";
  if (kind.includes("漏译")) return "#ffd479";
  return "#ffb08a";
};

const TYPE_ORDER: Record<string, number> = { "电影": 0, "动漫": 1, "剧集": 2 };
const TYPE_LABEL: Record<string, string> = { "电影": "电影", "动漫": "动画", "剧集": "剧集" };
const TYPE_CLASS: Record<string, string> = { "电影": "movie", "动漫": "anime", "剧集": "tv" };

function subDir(categoryPath: string): string {
  const segs = categoryPath.split("/").filter(Boolean);
  return segs.slice(1).join("/");
}

/** 渲染未命中海报的表格：排序（类型→目录→名称）+ 单行省略 + 悬停看全文。 */
function renderPosterTable(failed: FailedItem[]): string {
  if (!failed.length) {
    return `<div style="color:var(--text-dim);font-size:12px">全部命中，无需处理。</div>`;
  }
  const rows = [...failed].sort((a, b) => {
    const t = (TYPE_ORDER[a.category] ?? 9) - (TYPE_ORDER[b.category] ?? 9);
    if (t !== 0) return t;
    const d = subDir(a.category_path).localeCompare(subDir(b.category_path), "zh");
    if (d !== 0) return d;
    return a.title.localeCompare(b.title, "zh");
  });
  const body = rows.map((f, i) => {
    const dir = subDir(f.category_path);
    const label = TYPE_LABEL[f.category] ?? f.category;
    const cls = TYPE_CLASS[f.category] ?? "movie";
    let suggHtml: string;
    let suggTitle: string;
    if (f.suggest_name) {
      suggHtml = `<span class="pt-note">${esc(f.suggest_note)}</span> <span class="pt-pill">${esc(f.suggest_name)}</span>`;
      suggTitle = `${f.suggest_note} → ${f.suggest_name}`;
    } else {
      suggHtml = `<span class="pt-warn">${esc(f.suggest_note)}</span>`;
      suggTitle = f.suggest_note;
    }
    return `<tr>
      <td class="pt-idx">${i + 1}</td>
      <td><span class="pt-tag ${cls}">${esc(label)}</span></td>
      <td class="pt-dir" title="${esc(dir)}">${esc(dir)}</td>
      <td class="pt-name" title="${esc(f.title)}">${esc(f.title)}</td>
      <td class="pt-sugg" title="${esc(suggTitle)}">${suggHtml}</td>
    </tr>`;
  }).join("");
  return `<div class="pt-wrap"><table class="poster-table">
    <colgroup><col class="pt-c-idx"><col class="pt-c-type"><col class="pt-c-dir"><col class="pt-c-name"><col class="pt-c-sugg"></colgroup>
    <thead><tr><th>#</th><th>类型</th><th>目录</th><th>名称</th><th>优化建议</th></tr></thead>
    <tbody>${body}</tbody>
  </table></div>`;
}

// 最近一次字幕报告的结构化数据（供单文件重校刷新）。
let lastSubReports: SubReport[] = [];

function renderSubReport(reports: SubReport[]): string {
  const totalIssues = reports.reduce((a, r) => a + r.issues.length, 0);
  const countBadge = totalIssues
    ? `<span class="sub-count-badge has">${totalIssues} 条提示</span>`
    : `<span class="sub-count-badge none">无质检问题</span>`;
  let html = `<div class="sub-summary"><span>✓ 处理 ${reports.length} 个文件</span>${countBadge}</div>`;
  reports.filter(r => r.issues.length).forEach(r => {
    const issuesHtml = r.issues.map(i => {
      const editable = i.src_lines && i.src_lines.length > 0;
      const srcAttr = editable ? i.src_lines.join(",") : "";
      const hint = editable ? ' title="双击编辑"' : ' title="该文件无法解码，不能编辑"';
      return `<div class="sub-issue${editable ? " editable" : ""}" data-file="${esc(r.file)}" data-src-lines="${srcAttr}" data-kind="${esc(i.kind)}"${hint}><span class="sub-kind" style="--k:${kindColor(i.kind)}">${esc(i.kind)}</span><span class="sub-loc">L${i.line}</span><span class="sub-text">${esc(i.text)}</span></div>`;
    }).join("");
    html += `<details class="sub-file"><summary><span class="sub-fname">${esc(r.file)}</span><span class="sub-badge">${r.issues.length}</span></summary><div class="sub-issues">${issuesHtml}</div></details>`;
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

export async function startPoster(): Promise<void> {
  if (!begin("poster")) return;
  const s = states.poster;
  try {
    const report = await api.fetchPosters();
    s.pct = 100;
    s.statusText = `完成：成功 ${report.ok}，未命中 ${report.failed.length}`;
    s.resultHtml = renderPosterTable(report.failed);
    s.status = "done";
  } catch (err) {
    s.statusText = "更新失败：" + String(err);
    s.status = "error";
  }
  notify();
}

export async function startSubtitle(dir: string): Promise<void> {
  if (!begin("subtitle")) return;
  const s = states.subtitle;
  try {
    const reports = await api.normalizeSubtitles(dir);
    lastSubReports = reports;
    s.pct = 100;
    s.statusText = `完成：处理 ${reports.length} 个文件`;
    s.resultHtml = renderSubReport(reports);
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

/** 保存编辑后：用重校得到的新 issues 替换指定 file 那一组，重渲染字幕结果并通知。 */
export function updateFileIssues(file: string, issues: SubReport["issues"]): void {
  const idx = lastSubReports.findIndex((r) => r.file === file);
  if (idx >= 0) lastSubReports[idx] = { file, issues };
  const s = states.subtitle;
  s.resultHtml = renderSubReport(lastSubReports);
  notify();
}
