import { open, save } from "@tauri-apps/plugin-dialog";
import { listen } from "@tauri-apps/api/event";
import { api } from "../lib/ipc";
import type { SubReport, FailedItem } from "../lib/ipc";
import { esc } from "../lib/escape";
import { icon } from "../lib/icons";

/** 按质检提示类型上色。 */
const kindColor = (kind: string): string => {
  if (kind.includes("交叉")) return "#ff9b9b";
  if (kind.includes("未合并") || kind.includes("多于")) return "#ffb07a";
  if (kind.includes("对话")) return "#9db8ff";
  if (kind.includes("道具") || kind.includes("外语") || kind.includes("歌曲")) return "#c3a8ff";
  return "#ffb08a";
};

export function NormalizeView(): HTMLElement {
  const el = document.createElement("div");
  el.className = "view-enter";
  el.innerHTML = `
    <h1 style="font-size:20px;margin-bottom:16px">工具箱</h1>
    <div class="glass" style="padding:16px;margin-bottom:16px">
      <h3 style="margin-bottom:10px">漫画标准化</h3>
      <div style="display:flex;gap:10px;flex-wrap:wrap;align-items:center">
        <button class="icon-text" id="c-dir">${icon("folder", 15)}<span class="btn-label">选择图片目录</span></button><span id="c-dir-p" style="color:var(--text-dim);font-size:12px">未选</span>
        <input id="c-prefix" placeholder="命名前缀，如 海贼王01" style="padding:6px"/>
        <button class="icon-text" id="c-out">${icon("folder", 15)}<span class="btn-label">选择输出zip</span></button><span id="c-out-p" style="color:var(--text-dim);font-size:12px">未选</span>
        <button class="icon-text" id="c-run">${icon("play", 15)}<span class="btn-label">开始</span></button>
      </div>
      <div id="c-report" style="margin-top:12px;color:var(--text-dim)"></div>
    </div>
    <div class="glass setting-card">
      <div class="setting-card-head"><span class="setting-card-title">影视海报生成</span></div>
      <div class="setting-row">
        <span class="setting-label">TMDB Key</span>
        <input id="tmdb-key" class="setting-path" style="flex:1;padding:6px 10px;border-radius:8px;background:var(--glass);border:1px solid var(--border);color:var(--text)" placeholder="填入 TMDB API Key" />
      </div>
      <div class="setting-actions">
        <button class="btn-primary icon-text" id="fetch-posters">${icon("refresh", 15)}<span class="btn-label">更新海报</span></button>
        <span id="poster-progress" style="color:var(--text-dim);font-size:12px"></span>
      </div>
      <div id="poster-result" style="margin-top:10px"></div>
    </div>
    <div class="glass setting-card">
      <div class="setting-card-head"><span class="setting-card-title">字幕批量校准</span></div>
      <div class="setting-row">
        <span class="setting-label">输入目录</span>
        <span id="sub-in-p" class="setting-path">docs/subtitles</span>
        <button class="icon-text" id="sub-in">${icon("folder", 15)}<span class="btn-label">选择</span></button>
      </div>
      <div class="setting-row">
        <span class="setting-label">输出</span>
        <span class="setting-path">应用字幕库（自动，按目录结构）</span>
      </div>
      <div class="setting-actions">
        <button class="btn-primary icon-text" id="sub-run" disabled>${icon("play", 15)}<span class="btn-label">开始校准</span></button>
      </div>
      <div id="sub-progress" style="display:none;margin-top:10px">
        <div style="height:6px;border-radius:4px;background:var(--glass);overflow:hidden">
          <div id="sub-progress-bar" style="height:100%;width:0%;background:var(--accent);transition:width .2s"></div>
        </div>
        <div id="sub-progress-text" style="font-size:12px;color:var(--text-dim);margin-top:4px"></div>
      </div>
      <div id="sub-report" style="margin-top:10px"></div>
    </div>`;

  let subIn = "", cDir = "", cOut = "";
  api.getSubtitleInputDir().then((d) => {
    if (d) {
      subIn = d;
      const p = el.querySelector("#sub-in-p"); if (p) p.textContent = d;
      const run = el.querySelector<HTMLButtonElement>("#sub-run"); if (run) run.disabled = false;
    }
  });
  const pick = async (setter: (v: string) => void, spanId: string) => {
    const d = await open({ directory: true });
    if (typeof d === "string") { setter(d); el.querySelector(`#${spanId}`)!.textContent = d; }
  };
  el.querySelector<HTMLButtonElement>("#sub-in")!.onclick = async () => {
    const d = await open({ directory: true });
    if (typeof d === "string") {
      subIn = d;
      el.querySelector("#sub-in-p")!.textContent = d;
      await api.setSubtitleInputDir(d);
      el.querySelector<HTMLButtonElement>("#sub-run")!.disabled = false;
    }
  };
  el.querySelector<HTMLButtonElement>("#c-dir")!.onclick = () => pick(v => cDir = v, "c-dir-p");
  el.querySelector<HTMLButtonElement>("#c-out")!.onclick = async () => {
    const f = await save({ filters: [{ name: "zip", extensions: ["zip"] }] });
    if (f) { cOut = f; el.querySelector("#c-out-p")!.textContent = f; }
  };

  el.querySelector<HTMLButtonElement>("#sub-run")!.onclick = async () => {
    const btn = el.querySelector<HTMLButtonElement>("#sub-run")!;
    const label = btn.querySelector<HTMLElement>(".btn-label")!;
    const prog = el.querySelector<HTMLElement>("#sub-progress")!;
    const bar = el.querySelector<HTMLElement>("#sub-progress-bar")!;
    const text = el.querySelector<HTMLElement>("#sub-progress-text")!;
    btn.disabled = true; label.textContent = "处理中…";
    el.querySelector("#sub-report")!.innerHTML = "";
    bar.style.width = "0%"; text.textContent = "准备中…（正在扫描目录）"; prog.style.display = "block";
    let unlistenSub: (() => void) | null = null;
    try {
      unlistenSub = await listen<{ done: number; total: number }>("subtitle-progress", (e) => {
        const { done, total } = e.payload;
        const pct = total ? Math.round((done / total) * 100) : 0;
        bar.style.width = pct + "%";
        text.textContent = `已处理 ${done}/${total}`;
      });
      const dir = subIn || "docs/subtitles";
      const reports: SubReport[] = await api.normalizeSubtitles(dir);
      bar.style.width = "100%";
      const totalIssues = reports.reduce((a, r) => a + r.issues.length, 0);
      // 报告：醒目汇总头 + 每个有问题文件折叠项（新结构见 theme.css）
      const box = el.querySelector("#sub-report")!;
      const countBadge = totalIssues
        ? `<span class="sub-count-badge has">${totalIssues} 条提示</span>`
        : `<span class="sub-count-badge none">无质检问题</span>`;
      let html = `<div class="sub-summary"><span>✓ 处理 ${reports.length} 个文件</span>${countBadge}</div>`;
      reports.filter(r => r.issues.length).forEach(r => {
        const issuesHtml = r.issues.map(i =>
          `<div class="sub-issue"><span class="sub-kind" style="--k:${kindColor(i.kind)}">${esc(i.kind)}</span><span class="sub-loc">L${i.line}</span><span class="sub-text" title="${esc(i.text)}">${esc(i.text)}</span></div>`
        ).join("");
        html += `<details class="sub-file"><summary><span class="sub-fname">${esc(r.file)}</span><span class="sub-badge">${r.issues.length}</span></summary><div class="sub-issues">${issuesHtml}</div></details>`;
      });
      box.innerHTML = html;
    } catch (e) {
      alert("字幕批量校准失败：" + e);
    } finally {
      if (unlistenSub) { unlistenSub(); unlistenSub = null; }
      btn.disabled = false; label.textContent = "开始校准";
    }
  };
  el.querySelector<HTMLButtonElement>("#c-run")!.onclick = async () => {
    const prefix = (el.querySelector("#c-prefix") as HTMLInputElement).value.trim();
    if (!cDir || !cOut || !prefix) { alert("请选择目录、前缀和输出zip"); return; }
    const btn = el.querySelector<HTMLButtonElement>("#c-run")!;
    const label = btn.querySelector<HTMLElement>(".btn-label")!;
    btn.disabled = true; label.textContent = "处理中…";
    try {
      const n = await api.normalizeComic(cDir, prefix, cOut);
      el.querySelector("#c-report")!.textContent = `完成：${n} 页已打包`;
    } catch (e) {
      el.querySelector("#c-report")!.textContent = "漫画标准化失败：" + e;
    } finally {
      btn.disabled = false; label.textContent = "开始";
    }
  };
  // 海报：预填 Key + 保存 + 抓取
  const keyInput = el.querySelector<HTMLInputElement>("#tmdb-key")!;
  api.getTmdbKey().then(k => { if (k) keyInput.value = k; });
  keyInput.onchange = () => { api.setTmdbKey(keyInput.value.trim()); };

  const fetchBtn = el.querySelector<HTMLButtonElement>("#fetch-posters")!;
  const progressEl = el.querySelector<HTMLElement>("#poster-progress")!;
  const resultEl = el.querySelector<HTMLElement>("#poster-result")!;
  let unlisten: (() => void) | null = null;

  fetchBtn.onclick = async () => {
    await api.setTmdbKey(keyInput.value.trim());
    fetchBtn.disabled = true;
    resultEl.innerHTML = "";
    progressEl.textContent = "准备中…";
    unlisten = await listen<{ done: number; total: number; current_title: string }>(
      "poster-progress",
      (e) => { progressEl.textContent = `抓取中… ${e.payload.done}/${e.payload.total}`; }
    );
    try {
      const report = await api.fetchPosters();
      progressEl.textContent = `完成：成功 ${report.ok}，未命中 ${report.failed.length}`;
      resultEl.innerHTML = renderPosterTable(report.failed);
    } catch (err) {
      progressEl.textContent = "更新失败：" + String(err);
    } finally {
      fetchBtn.disabled = false;
      if (unlisten) { unlisten(); unlisten = null; }
    }
  };

  return el;
}

// 类型显示名与排序权重（电影>动画>剧集）
const TYPE_ORDER: Record<string, number> = { "电影": 0, "动漫": 1, "剧集": 2 };
const TYPE_LABEL: Record<string, string> = { "电影": "电影", "动漫": "动画", "剧集": "剧集" };
const TYPE_CLASS: Record<string, string> = { "电影": "movie", "动漫": "anime", "剧集": "tv" };

/** 目录去掉首级分类（电影/动漫/剧集），从下一级起。 */
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
    // 优化建议：有推荐名 → 说明 + 绿色药丸高亮推荐名；无 → 橙色警示说明
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
