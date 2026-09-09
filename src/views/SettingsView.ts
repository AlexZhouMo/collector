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
      <div class="setting-actions">
        <button class="btn-primary icon-text" id="migrate-rel">${icon("refresh", 15)}<span class="btn-label">迁移为相对路径</span></button>
        <span id="migrate-msg" style="color:var(--text-dim);font-size:12px"></span>
      </div>
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

  // 迁移为相对路径（一次性，不可逆——执行前建议手动备份数据库）
  const migBtn = el.querySelector<HTMLButtonElement>("#migrate-rel")!;
  const migMsg = el.querySelector<HTMLElement>("#migrate-msg")!;
  migBtn.onclick = async () => {
    if (!confirm("将把库中路径迁移为相对存储并重置 ID（不可逆）。建议先备份数据库。确定继续？")) return;
    migBtn.disabled = true;
    const label = migBtn.querySelector<HTMLElement>(".btn-label")!;
    const orig = label.textContent;
    label.textContent = "迁移中…";
    try {
      const n = await api.migrateToRelative();
      migMsg.textContent = `迁移完成，${n} 条`;
    } catch (e) {
      migMsg.textContent = "迁移失败：" + String(e);
    } finally {
      migBtn.disabled = false;
      label.textContent = orig;
    }
  };

  return el;
}
