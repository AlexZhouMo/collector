import { open } from "@tauri-apps/plugin-dialog";
import { listen } from "@tauri-apps/api/event";
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

  const singleCards = SINGLE_KINDS.map(
    ([k, label]) => `
      <div class="glass setting-card">
        <div class="setting-card-head">
          <span class="setting-card-title">${label}</span>
          <span id="root-${k}" class="setting-path">${esc(singleRoots[k] ?? "未设置")}</span>
        </div>
        <div class="setting-actions">
          <button class="icon-text" data-pick="${k}">${icon("folder", 15)}<span class="btn-label">选择目录</span></button>
          <button class="btn-primary icon-text" data-scan="${k}">${icon("refresh", 15)}<span class="btn-label">扫描</span></button>
        </div>
      </div>`
  ).join("");

  el.innerHTML = `
    <h1 style="font-size:20px;margin-bottom:16px">设置</h1>
    <div class="glass setting-card">
      <div class="setting-card-head"><span class="setting-card-title">视频</span></div>
      ${videoRows}
      <div class="setting-actions">
        <button class="btn-primary icon-text" id="init-demo">${icon("refresh", 15)}<span class="btn-label">初始化示例库</span></button>
      </div>
    </div>
    <div class="glass setting-card">
      <div class="setting-card-head"><span class="setting-card-title">海报</span></div>
      <div class="setting-row">
        <span class="setting-label">TMDB Key</span>
        <input id="tmdb-key" class="setting-path" style="flex:1;padding:6px 10px;border-radius:8px;background:var(--glass);border:1px solid var(--border);color:var(--text)" placeholder="填入 TMDB API Key" />
      </div>
      <div class="setting-actions">
        <button class="btn-primary icon-text" id="fetch-posters">${icon("refresh", 15)}<span class="btn-label">抓取缺失海报</span></button>
        <span id="poster-progress" style="color:var(--text-dim);font-size:12px"></span>
      </div>
      <div id="poster-result" style="margin-top:8px;font-size:12px;color:var(--text-dim);max-height:220px;overflow:auto"></div>
    </div>
    ${singleCards}`;

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

  // 初始化示例库：直接用项目内固定的 demo/subtitles 路径导入（开发期），
  // 不弹目录选择，避免选错层级导致导入 0 条。
  const DEMO_ROOT = "/Users/zhoumo/Documents/Claude/collector/demo/subtitles";
  const initBtn = el.querySelector<HTMLButtonElement>("#init-demo")!;
  initBtn.onclick = async () => {
    initBtn.disabled = true;
    const label = initBtn.querySelector<HTMLElement>(".btn-label")!;
    const orig = label.textContent;
    label.textContent = "导入中…";
    try {
      const n = await api.initFromDemo(DEMO_ROOT);
      alert(`初始化导入完成，${n} 个视频`);
    } catch (e) {
      alert("导入失败：" + e);
    } finally {
      initBtn.disabled = false;
      label.textContent = orig;
    }
  };

  // 单目录素材扫描
  el.querySelectorAll<HTMLButtonElement>("[data-scan]").forEach((b) => {
    b.onclick = async () => {
      b.disabled = true;
      const label = b.querySelector<HTMLElement>(".btn-label")!;
      const original = label.textContent;
      label.textContent = "扫描中…";
      try {
        const n = await api.scanRoot(b.dataset.scan!);
        alert(`扫描完成，${n} 项`);
      } catch (e) {
        alert("扫描失败：" + e);
      } finally {
        b.disabled = false;
        label.textContent = original;
      }
    };
  });

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
      progressEl.textContent = `完成：成功 ${report.ok}，失败 ${report.failed.length}`;
      if (report.failed.length) {
        resultEl.innerHTML = "<div style='margin-bottom:4px'>失败清单：</div>" +
          report.failed.map(f => `<div>· ${f.title} — ${f.reason}</div>`).join("");
      }
    } catch (err) {
      progressEl.textContent = "抓取失败：" + String(err);
    } finally {
      fetchBtn.disabled = false;
      if (unlisten) { unlisten(); unlisten = null; }
    }
  };

  return el;
}
