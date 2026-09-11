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

  // 单目录素材扫描
  el.querySelectorAll<HTMLButtonElement>("[data-scan]").forEach((b) => {
    b.onclick = async () => {
      const kind = b.dataset.scan!;
      // 漫画重扫会清空并按磁盘（Vol_XX.zip 目录）重建，会删掉手动导入、
      // 磁盘上无对应压缩包的漫画记录。加二次确认防止误操作丢数据。
      if (kind === "comic" && !confirm(
        "重扫漫画会清空当前漫画库，并仅按磁盘上的 Vol_XX.zip 重新建立。\n" +
        "手动导入、磁盘上没有对应压缩包的漫画记录将被删除，且无法恢复。\n\n" +
        "确定要继续吗？"
      )) return;
      b.disabled = true;
      const label = b.querySelector<HTMLElement>(".btn-label")!;
      const original = label.textContent;
      label.textContent = "扫描中…";
      try {
        const n = await api.scanRoot(kind);
        alert(`扫描完成，${n} 项`);
      } catch (e) {
        alert("扫描失败：" + e);
      } finally {
        b.disabled = false;
        label.textContent = original;
      }
    };
  });



  return el;
}
