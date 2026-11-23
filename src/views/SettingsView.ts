import { open, save } from "@tauri-apps/plugin-dialog";
import { listen } from "@tauri-apps/api/event";
import { api, type Service, type TransferKey } from "../lib/ipc";
import { showToast } from "../components/Toast";
import { progressBar } from "../components/ProgressBar";
import { esc } from "../lib/escape";
import { icon } from "../lib/icons";

// 视频三分类：settings key 前缀（后端拼 _root）+ 显示名
const VIDEO_CATS: [string, string][] = [
  ["video_movie", "电影"],
  ["video_anime", "动漫"],
  ["video_tv", "剧集"],
];
// 单目录素材：kind + 显示名
const SINGLE_KINDS: [string, string][] = [
  ["comic", "漫画"],
  ["game", "游戏"],
];
// 支持导入/导出数据包的分类
const TRANSFER_KEYS = new Set<string>(["video_movie", "video_anime", "video_tv", "comic", "game"]);
const transferBtns = (k: string): string =>
  TRANSFER_KEYS.has(k)
    ? `<button class="icon-text" data-import="${k}">${icon("upload", 15)}<span class="btn-label">导入</span></button>
        <button class="icon-text" data-export="${k}">${icon("download", 15)}<span class="btn-label">导出</span></button>`
    : "";
// 导入/导出进度条（与工具箱进度条同款），执行时显示在该分类行下方
const transferProgress = (k: string): string =>
  TRANSFER_KEYS.has(k) ? progressBar(`tp-${k}`, "margin:2px 0 10px") : "";

/** 外部链接小箭头（↗），表示会在浏览器中打开 */
const EXT_ICON = `<svg class="icon" viewBox="0 0 16 16" width="15" height="15" aria-hidden="true"><path d="M6 3.5H3.5v9h9V10M9 3.5h3.5V7M12.5 3.5L7 9" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round"/></svg>`;

// 服务管理：服务 → 显示名（Key 输入框 / 注册按钮的 id 前缀即服务名）
const SERVICES: [Service, string][] = [
  ["tmdb", "TMDB"],
  ["assrt", "射手网"],
];
const serviceRows = SERVICES.map(
  ([svc, name]) => `
      <div class="setting-row">
        <span class="setting-label">${name} Key</span>
        <input id="${svc}-key" class="input" type="text" spellcheck="false" autocomplete="off" placeholder="填入${name} API Key" />
        <button type="button" class="icon-text" id="${svc}-signup" title="在浏览器中打开${name}注册页">${EXT_ICON}<span class="btn-label">注册 ${name}</span></button>
      </div>`
).join("");

export async function SettingsView(): Promise<HTMLElement> {
  const el = document.createElement("div");
  el.className = "view-enter";

  // 预取当前所有已配置路径
  const videoRoots = Object.fromEntries(
    await Promise.all(VIDEO_CATS.map(async ([k]) => [k, await api.getRoot(k)]))
  );
  const singleRoots = Object.fromEntries(
    await Promise.all(SINGLE_KINDS.map(async ([k]) => [k, await api.getRoot(k)]))
  );

  const rootRow = (roots: Record<string, string | null>) => ([k, label]: [string, string]) => `
      <div class="setting-row">
        <span class="setting-label">${label}</span>
        <span id="root-${k}" class="setting-path">${esc(roots[k] ?? "未设置")}</span>
        <button class="icon-text" data-pick="${k}">${icon("folder", 15)}<span class="btn-label">选择目录</span></button>
        ${transferBtns(k)}
      </div>${transferProgress(k)}`;
  const videoRows = VIDEO_CATS.map(rootRow(videoRoots)).join("");
  const singleRows = SINGLE_KINDS.map(rootRow(singleRoots)).join("");

  el.innerHTML = `
    <h1 class="page-title">设置</h1>
    <div class="glass setting-card">
      <div class="setting-card-head"><span class="setting-card-title">路径设置</span></div>
      ${videoRows}
      ${singleRows}
    </div>
    <div class="glass setting-card">
      <div class="setting-card-head"><span class="setting-card-title">服务管理</span></div>
      ${serviceRows}
    </div>
    <div class="glass setting-card">
      <div class="setting-card-head">
        <span class="setting-card-title">视频缓存</span>
        <button class="icon-text" id="cache-clear-btn">${icon("trash", 15)}<span class="btn-label">清空缓存</span></button>
      </div>
      <div class="cache-meter">
        <div class="cache-meter-top">
          <span id="cache-meter-used" class="cache-meter-used">加载中…</span>
          <span id="cache-meter-limit" class="cache-meter-limit"></span>
        </div>
        <div class="cache-bar"><div id="cache-bar-fill" class="cache-bar-fill" style="width:0%"></div></div>
      </div>
      <div class="setting-row">
        <span class="setting-label">缓存目录</span>
        <span id="cache-path" class="setting-path">加载中…</span>
      </div>
    </div>`;

  // 选目录（视频三分类 + 单目录素材共用同一套逻辑：key/kind 存到 data-pick）
  el.querySelectorAll<HTMLButtonElement>("[data-pick]").forEach((b) => {
    b.onclick = async () => {
      const k = b.dataset.pick!;
      const dir = await open({ directory: true });
      if (typeof dir === "string") {
        await api.setRoot(k, dir);
        el.querySelector(`#root-${k}`)!.textContent = dir;
      }
    };
  });

  // 服务管理：各服务的 API Key 失焦/回车时保存；按钮打开该服务注册页
  SERVICES.forEach(([svc, name]) => {
    const input = el.querySelector<HTMLInputElement>(`#${svc}-key`)!;
    el.querySelector<HTMLButtonElement>(`#${svc}-signup`)!.onclick = () =>
      api.openServiceSignup(svc).catch((e) => showToast("无法打开浏览器：" + e, "error"));
    api.getServiceKey(svc).then((k) => { if (k) input.value = k; }).catch(() => {});
    input.onchange = () => {
      api.setServiceKey(svc, input.value.trim())
        .then(() => showToast(`${name} Key 已保存`))
        .catch((e) => showToast("保存失败：" + e, "error"));
    };
  });

  // 分类数据包：导出 zip / 从 zip 导入（导入会清空该分类并重新编号）
  const labelOf = (k: string): string =>
    [...VIDEO_CATS, ...SINGLE_KINDS].find(([key]) => key === k)?.[1] ?? k;
  const today = (): string => {
    const d = new Date();
    const p = (n: number) => String(n).padStart(2, "0");
    return `${d.getFullYear()}${p(d.getMonth() + 1)}${p(d.getDate())}`;
  };
  const runBusy = async (b: HTMLButtonElement, task: () => Promise<void>) => {
    const all = el.querySelectorAll<HTMLButtonElement>("[data-import],[data-export]");
    all.forEach((x) => (x.disabled = true));
    try {
      await task();
    } catch (e) {
      showToast(String(e), "error");
    } finally {
      all.forEach((x) => (x.disabled = false));
      b.blur();
    }
  };
  // 执行后端导入/导出期间显示该分类的进度条，收 transfer-progress 事件推进；结束后隐藏。
  const withProgress = async <T,>(k: TransferKey, op: string, task: () => Promise<T>): Promise<T> => {
    const box = el.querySelector<HTMLElement>(`#tp-${k}`);
    const bar = el.querySelector<HTMLElement>(`#tp-${k}-bar`);
    const text = el.querySelector<HTMLElement>(`#tp-${k}-text`);
    const set = (pct: number, stage: string) => {
      if (bar) bar.style.width = `${pct}%`;
      if (text) text.textContent = `${op}中… ${stage} ${pct}%`;
    };
    set(0, "准备");
    if (box) box.style.display = "";
    const unlisten = await listen<{ key: string; stage: string; pct: number }>("transfer-progress", (e) => {
      if (e.payload.key === k) set(e.payload.pct, e.payload.stage);
    });
    try {
      return await task();
    } finally {
      unlisten();
      if (box) box.style.display = "none";
    }
  };
  el.querySelectorAll<HTMLButtonElement>("[data-export]").forEach((b) => {
    b.onclick = () => runBusy(b, async () => {
      const k = b.dataset.export as TransferKey;
      const label = labelOf(k);
      const dest = await save({
        defaultPath: `collector-${label}-${today()}.zip`,
        filters: [{ name: "Collector 数据包", extensions: ["zip"] }],
      });
      if (typeof dest !== "string") return;
      const r = await withProgress(k, "导出", () => api.categoryExport(k, dest));
      const miss = r.missing_files.length ? `，${r.missing_files.length} 个文件缺失未打包` : "";
      showToast(`已导出「${label}」：${r.rows} 条、封面 ${r.covers}、字幕 ${r.subtitles}${miss}`);
    });
  });
  el.querySelectorAll<HTMLButtonElement>("[data-import]").forEach((b) => {
    b.onclick = () => runBusy(b, async () => {
      const k = b.dataset.import as TransferKey;
      const label = labelOf(k);
      const src = await open({
        multiple: false,
        filters: [{ name: "Collector 数据包", extensions: ["zip"] }],
      });
      if (typeof src !== "string") return;
      if (!confirm(`导入将清空「${label}」现有的全部数据，并用数据包内容替换（ID 重新编号）。确定继续？`)) return;
      const r = await withProgress(k, "导入", () => api.categoryImport(k, src));
      const ren = r.renamed_covers.length ? `，${r.renamed_covers.length} 个同名封面已改名` : "";
      showToast(`已导入「${label}」：${r.rows} 条、封面 ${r.covers}、字幕 ${r.subtitles}${ren}`);
    });
  });

  // 视频缓存：图形化使用率（进度条 + 数值），支持清空。
  const fmtBytes = (n: number): string => {
    if (n <= 0) return "0 B";
    const units = ["B", "KB", "MB", "GB", "TB"];
    const i = Math.min(units.length - 1, Math.floor(Math.log(n) / Math.log(1024)));
    return `${(n / Math.pow(1024, i)).toFixed(i === 0 ? 0 : 1)} ${units[i]}`;
  };
  const cachePathEl = el.querySelector<HTMLElement>("#cache-path")!;
  const usedEl = el.querySelector<HTMLElement>("#cache-meter-used")!;
  const limitEl = el.querySelector<HTMLElement>("#cache-meter-limit")!;
  const barFill = el.querySelector<HTMLElement>("#cache-bar-fill")!;
  const cacheClearBtn = el.querySelector<HTMLButtonElement>("#cache-clear-btn")!;
  const loadCacheInfo = async () => {
    try {
      const info = await api.cacheInfo();
      cachePathEl.textContent = info.path;
      cachePathEl.title = info.path;
      const ratio = info.limit_bytes > 0 ? info.used_bytes / info.limit_bytes : 0;
      const pct = Math.round(ratio * 100);
      usedEl.textContent = `${fmtBytes(info.used_bytes)}　${pct}%`;
      limitEl.textContent = `上限 ${fmtBytes(info.limit_bytes)}`;
      // 进度条：宽度至少留 2% 可见（非空时），高占比转警示色
      barFill.style.width = `${Math.max(ratio > 0 ? 2 : 0, Math.min(100, ratio * 100))}%`;
      barFill.classList.toggle("warn", ratio >= 0.85);
      barFill.classList.toggle("caution", ratio >= 0.6 && ratio < 0.85);
    } catch (e) {
      cachePathEl.textContent = "读取失败";
      usedEl.textContent = String(e);
      limitEl.textContent = "";
    }
  };
  loadCacheInfo();
  cacheClearBtn.onclick = async () => {
    if (!confirm("确定清空视频缓存？已转码的播放缓存将被删除，下次播放需重新转码。")) return;
    cacheClearBtn.disabled = true;
    const prevUsed = usedEl.textContent;
    usedEl.textContent = "清空中…";
    try {
      const removed = await api.cacheClear();
      await loadCacheInfo();
      // 与导入/导出相同的 toast 提示
      showToast(`已清空缓存：删除 ${removed} 个文件。`);
    } catch (e) {
      usedEl.textContent = prevUsed;
      showToast("清空缓存失败：" + String(e), "error");
    } finally {
      cacheClearBtn.disabled = false;
    }
  };

  return el;
}
