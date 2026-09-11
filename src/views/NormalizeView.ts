import { open } from "@tauri-apps/plugin-dialog";
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
    <div class="glass setting-card">
      <div class="setting-card-head"><span class="setting-card-title">影视海报生成</span></div>
      <div class="setting-row">
        <span class="setting-label">TMDB Key</span>
        <input id="tmdb-key" class="setting-path" style="flex:1;padding:6px 10px;border-radius:8px;background:var(--glass);border:1px solid var(--border);color:var(--text)" placeholder="填入 TMDB API Key" />
      </div>
      <div class="setting-actions">
        <button class="btn-primary icon-text" id="fetch-posters">${icon("refresh", 15)}<span class="btn-label">更新海报</span></button>
      </div>
      <div id="poster-progress" style="display:none;margin-top:10px">
        <div style="height:6px;border-radius:4px;background:var(--glass);overflow:hidden">
          <div id="poster-progress-bar" style="height:100%;width:0%;background:var(--accent);transition:width .2s"></div>
        </div>
        <div id="poster-progress-text" style="font-size:12px;color:var(--text-dim);margin-top:4px"></div>
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
    </div>
    <div class="glass setting-card">
      <div class="setting-card-head"><span class="setting-card-title">漫画自动归档</span></div>
      <div class="setting-row">
        <span class="setting-label">漫画目录</span>
        <span id="comic-dir-p" class="setting-path">未配置</span>
      </div>
      <div class="setting-actions">
        <button class="btn-primary icon-text" id="comic-run" disabled>${icon("play", 15)}<span class="btn-label">开始归档</span></button>
      </div>
      <div id="comic-progress" style="display:none;margin-top:10px">
        <div style="height:6px;border-radius:4px;background:var(--glass);overflow:hidden">
          <div id="comic-progress-bar" style="height:100%;width:0%;background:var(--accent);transition:width .2s"></div>
        </div>
        <div id="comic-progress-text" style="font-size:12px;color:var(--text-dim);margin-top:4px"></div>
      </div>
      <div id="comic-report" style="margin-top:10px"></div>
    </div>
    <div class="glass setting-card">
      <div class="setting-card-head"><span class="setting-card-title">漫画封面拓取</span></div>
      <div class="setting-row">
        <span class="setting-path">从 AniList 为无封面的漫画自动拓取封面</span>
      </div>
      <div class="setting-actions">
        <button class="btn-primary icon-text" id="manga-cover-run">${icon("refresh", 15)}<span class="btn-label">拓取封面</span></button>
      </div>
      <div id="manga-cover-progress" style="display:none;margin-top:10px">
        <div style="height:6px;border-radius:4px;background:var(--glass);overflow:hidden">
          <div id="manga-cover-bar" style="height:100%;width:0%;background:var(--accent);transition:width .2s"></div>
        </div>
        <div id="manga-cover-text" style="font-size:12px;color:var(--text-dim);margin-top:4px"></div>
      </div>
      <div id="manga-cover-result" style="margin-top:10px"></div>
    </div>`;

  let subIn = "";
  api.getSubtitleInputDir().then((d) => {
    if (d) {
      subIn = d;
      const p = el.querySelector("#sub-in-p"); if (p) p.textContent = d;
      const run = el.querySelector<HTMLButtonElement>("#sub-run"); if (run) run.disabled = false;
    }
  });
  api.getRoot("comic").then((d) => {
    const p = el.querySelector("#comic-dir-p");
    const run = el.querySelector<HTMLButtonElement>("#comic-run");
    if (d) {
      if (p) p.textContent = d;
      if (run) run.disabled = false;
    } else {
      if (p) p.textContent = "请先在设置中配置漫画根目录";
    }
  });
  el.querySelector<HTMLButtonElement>("#sub-in")!.onclick = async () => {
    const d = await open({ directory: true });
    if (typeof d === "string") {
      subIn = d;
      el.querySelector("#sub-in-p")!.textContent = d;
      await api.setSubtitleInputDir(d);
      el.querySelector<HTMLButtonElement>("#sub-run")!.disabled = false;
    }
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
      // 强制让 WKWebView 先绘制进度条一帧，再发起耗时的后端调用——否则
      // display:block 的变更会被紧随其后的阻塞 IPC 挡住，进度条要等后端返回才出现。
      await new Promise<void>((r) => requestAnimationFrame(() => requestAnimationFrame(() => r())));
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
  el.querySelector<HTMLButtonElement>("#comic-run")!.onclick = async () => {
    const btn = el.querySelector<HTMLButtonElement>("#comic-run")!;
    const label = btn.querySelector<HTMLElement>(".btn-label")!;
    const prog = el.querySelector<HTMLElement>("#comic-progress")!;
    const bar = el.querySelector<HTMLElement>("#comic-progress-bar")!;
    const text = el.querySelector<HTMLElement>("#comic-progress-text")!;
    btn.disabled = true; label.textContent = "归档中…";
    el.querySelector("#comic-report")!.innerHTML = "";
    bar.style.width = "0%"; text.textContent = "准备中…（正在扫描目录）"; prog.style.display = "block";
    let unlistenC: (() => void) | null = null;
    try {
      unlistenC = await listen<{ manga: string; vol: string; done_images: number; total_images: number }>("comic-archive-progress", (e) => {
        const { manga, vol, done_images, total_images } = e.payload;
        const pct = total_images ? Math.round((done_images / total_images) * 100) : 0;
        bar.style.width = pct + "%";
        text.textContent = manga
          ? `正在：${manga} ${vol}｜图片 ${done_images}/${total_images}`
          : `准备中…`;
      });
      await new Promise<void>((r) => requestAnimationFrame(() => requestAnimationFrame(() => r())));
      const reports = await api.archiveComics();
      bar.style.width = "100%";
      const ok = reports.filter(r => r.status === "成功").length;
      const skip = reports.filter(r => r.status.includes("跳过")).length;
      const fail = reports.filter(r => r.status.startsWith("失败")).length;
      const box = el.querySelector("#comic-report")!;
      const badge = `<span class="sub-count-badge ${fail ? "has" : "none"}">成功 ${ok}・跳过 ${skip}・失败 ${fail}</span>`;
      let html = `<div class="sub-summary"><span>✓ 处理 ${reports.length} 卷</span>${badge}</div>`;
      const byManga = new Map<string, typeof reports>();
      reports.forEach(r => { const a = byManga.get(r.manga) ?? []; a.push(r); byManga.set(r.manga, a); });
      byManga.forEach((vols, manga) => {
        const rows = vols.map(v => {
          const color = v.status === "成功" ? "#8fdca0" : v.status.includes("跳过") ? "#9db8ff" : "#ff9b9b";
          const extra = v.status === "成功" ? `${v.pages} 页` : v.status;
          return `<div class="sub-issue"><span class="sub-kind" style="--k:${color}">${esc(v.vol)}</span><span class="sub-text">${esc(extra)}</span></div>`;
        }).join("");
        html += `<details class="sub-file" open><summary><span class="sub-fname">${esc(manga)}</span><span class="sub-badge">${vols.length}</span></summary><div class="sub-issues">${rows}</div></details>`;
      });
      box.innerHTML = html;
      await api.scanRoot("comic");
    } catch (e) {
      alert("漫画自动归档失败：" + e);
    } finally {
      if (unlistenC) { unlistenC(); unlistenC = null; }
      btn.disabled = false; label.textContent = "开始归档";
    }
  };
  // 海报：预填 Key + 保存 + 抓取
  const keyInput = el.querySelector<HTMLInputElement>("#tmdb-key")!;
  api.getTmdbKey().then(k => { if (k) keyInput.value = k; });
  keyInput.onchange = () => { api.setTmdbKey(keyInput.value.trim()); };

  const fetchBtn = el.querySelector<HTMLButtonElement>("#fetch-posters")!;
  const posterProg = el.querySelector<HTMLElement>("#poster-progress")!;
  const posterBar = el.querySelector<HTMLElement>("#poster-progress-bar")!;
  const posterText = el.querySelector<HTMLElement>("#poster-progress-text")!;
  const resultEl = el.querySelector<HTMLElement>("#poster-result")!;
  let unlisten: (() => void) | null = null;

  fetchBtn.onclick = async () => {
    await api.setTmdbKey(keyInput.value.trim());
    const label = fetchBtn.querySelector<HTMLElement>(".btn-label")!;
    fetchBtn.disabled = true; label.textContent = "抓取中…";
    resultEl.innerHTML = "";
    posterBar.style.width = "0%"; posterText.textContent = "准备中…"; posterProg.style.display = "block";
    try {
      unlisten = await listen<{ done: number; total: number; current_title: string }>(
        "poster-progress",
        (e) => {
          const { done, total } = e.payload;
          const pct = total ? Math.round((done / total) * 100) : 0;
          posterBar.style.width = pct + "%";
          posterText.textContent = `抓取中… ${done}/${total}`;
        }
      );
      // 与字幕校准一致：先强制绘制进度条一帧，再发起耗时调用，避免被阻塞挡住。
      await new Promise<void>((r) => requestAnimationFrame(() => requestAnimationFrame(() => r())));
      const report = await api.fetchPosters();
      posterBar.style.width = "100%";
      posterText.textContent = `完成：成功 ${report.ok}，未命中 ${report.failed.length}`;
      resultEl.innerHTML = renderPosterTable(report.failed);
    } catch (err) {
      posterText.textContent = "更新失败：" + String(err);
    } finally {
      if (unlisten) { unlisten(); unlisten = null; }
      fetchBtn.disabled = false; label.textContent = "更新海报";
    }
  };

  // 漫画封面拓取（从 AniList 为无封面的漫画自动拓取封面），仿海报逻辑
  const mangaCoverBtn = el.querySelector<HTMLButtonElement>("#manga-cover-run")!;
  const mangaCoverProg = el.querySelector<HTMLElement>("#manga-cover-progress")!;
  const mangaCoverBar = el.querySelector<HTMLElement>("#manga-cover-bar")!;
  const mangaCoverText = el.querySelector<HTMLElement>("#manga-cover-text")!;
  const mangaCoverResult = el.querySelector<HTMLElement>("#manga-cover-result")!;
  let unlistenMangaCover: (() => void) | null = null;

  mangaCoverBtn.onclick = async () => {
    const label = mangaCoverBtn.querySelector<HTMLElement>(".btn-label")!;
    mangaCoverBtn.disabled = true; label.textContent = "拓取中…";
    mangaCoverResult.innerHTML = "";
    mangaCoverBar.style.width = "0%"; mangaCoverText.textContent = "准备中…"; mangaCoverProg.style.display = "block";
    try {
      unlistenMangaCover = await listen<{ done: number; total: number; current_title: string }>(
        "manga-cover-progress",
        (e) => {
          const { done, total, current_title } = e.payload;
          const pct = total ? Math.round((done / total) * 100) : 0;
          mangaCoverBar.style.width = pct + "%";
          mangaCoverText.textContent = current_title
            ? `拓取中… ${done}/${total}｜${current_title}`
            : `拓取中… ${done}/${total}`;
        }
      );
      // 与字幕校准一致：先强制绘制进度条一帧，再发起耗时调用，避免被阻塞挡住。
      await new Promise<void>((r) => requestAnimationFrame(() => requestAnimationFrame(() => r())));
      const report = await api.fetchMangaCovers();
      mangaCoverBar.style.width = "100%";
      mangaCoverText.textContent = `完成：成功 ${report.ok}，未命中 ${report.failed.length}`;
      mangaCoverResult.innerHTML = report.failed.length
        ? report.failed.map(f => `<div class="sub-issue"><span class="sub-text" title="${esc(f.title)}">${esc(f.title)}</span><span class="sub-loc" title="${esc(f.reason)}">${esc(f.reason)}</span></div>`).join("")
        : `<div style="color:var(--text-dim);font-size:12px">全部命中，无需处理。</div>`;
    } catch (err) {
      mangaCoverText.textContent = "拓取失败：" + String(err);
    } finally {
      if (unlistenMangaCover) { unlistenMangaCover(); unlistenMangaCover = null; }
      mangaCoverBtn.disabled = false; label.textContent = "拓取封面";
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
