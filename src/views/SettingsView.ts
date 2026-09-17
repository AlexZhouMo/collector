import { open } from "@tauri-apps/plugin-dialog";
import { api } from "../lib/ipc";
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

  const videoRows = VIDEO_CATS.map(
    ([k, label]) => `
      <div class="setting-row">
        <span class="setting-label">${label}</span>
        <span id="root-${k}" class="setting-path">${esc(videoRoots[k] ?? "未设置")}</span>
        <button class="icon-text" data-pick="${k}">${icon("folder", 15)}<span class="btn-label">选择目录</span></button>
      </div>`
  ).join("");

  const singleRows = SINGLE_KINDS.map(
    ([k, label]) => `
      <div class="setting-row">
        <span class="setting-label">${label}</span>
        <span id="root-${k}" class="setting-path">${esc(singleRoots[k] ?? "未设置")}</span>
        <button class="icon-text" data-pick="${k}">${icon("folder", 15)}<span class="btn-label">选择目录</span></button>
      </div>`
  ).join("");

  el.innerHTML = `
    <h1 style="font-size:20px;margin-bottom:16px">设置</h1>
    <div class="glass setting-card">
      <div class="setting-card-head"><span class="setting-card-title">视频</span></div>
      ${videoRows}
    </div>
    <div class="glass setting-card">
      <div class="setting-card-head"><span class="setting-card-title">其他</span></div>
      ${singleRows}
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
      usedEl.textContent = `已清空（删除 ${removed} 个文件）`;
      setTimeout(loadCacheInfo, 1500);
    } catch (e) {
      usedEl.textContent = "清空失败: " + String(e);
      setTimeout(() => { usedEl.textContent = prevUsed; }, 2000);
    } finally {
      cacheClearBtn.disabled = false;
    }
  };

  return el;
}
