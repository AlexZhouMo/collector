import { open } from "@tauri-apps/plugin-dialog";
import { invoke } from "@tauri-apps/api/core";
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
      <div class="setting-card-head"><span class="setting-card-title">实验（阶段 0.2 验证）</span></div>
      <div class="setting-row">
        <span class="setting-label">libmpv 嵌入</span>
        <span id="mpv-embed-status" class="setting-path">点击后播放测试 mkv 并透明化界面</span>
        <button class="icon-text" id="mpv-embed-btn">测试 mpv 嵌入</button>
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

  // 阶段 0.2 验证：触发 libmpv 嵌入播放，并把界面临时透明化让视频透出。
  const mpvBtn = el.querySelector<HTMLButtonElement>("#mpv-embed-btn");
  const mpvStatus = el.querySelector<HTMLElement>("#mpv-embed-status");
  if (mpvBtn) {
    mpvBtn.onclick = async () => {
      mpvBtn.disabled = true;
      if (mpvStatus) mpvStatus.textContent = "调用中...";
      try {
        await invoke<void>("mpv_embed_probe");
        // 让根容器与 body 透明，露出下层的 mpv NSView（WKWebView 已由后端设透明）。
        document.documentElement.style.background = "transparent";
        document.body.style.background = "transparent";
        const app = document.querySelector<HTMLElement>("#app");
        if (app) app.style.background = "transparent";
        if (mpvStatus) mpvStatus.textContent = "已触发：若可行应看到视频透出（真机确认）";
      } catch (e) {
        if (mpvStatus) mpvStatus.textContent = "失败: " + String(e);
      } finally {
        mpvBtn.disabled = false;
      }
    };
  }

  return el;
}
