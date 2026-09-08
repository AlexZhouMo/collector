import { api } from "../lib/ipc";
import type { MediaItem } from "../lib/ipc";
import { open } from "@tauri-apps/plugin-dialog";
import { convertFileSrc } from "@tauri-apps/api/core";
import { esc } from "../lib/escape";

/// 打开右侧滑入抽屉表单，用于新增(item=null)或编辑(item 有值)视频条目。
/// 保存成功后关闭抽屉并回调 onSaved()。取消/遮罩点击/Esc 关闭不保存。
export function openEditDrawer(item: MediaItem | null, onSaved: () => void): void {
  // 局部维护的可变字段（文件选择后更新）
  let path = item?.path ?? "";
  let subtitlePath = item?.subtitle_path ?? "";
  let coverPath = item?.cover_path ?? "";

  const overlay = document.createElement("div");
  overlay.className = "drawer-overlay";

  const drawer = document.createElement("div");
  drawer.className = "edit-drawer glass";

  const category = item?.category ?? "电影";
  drawer.innerHTML = `
    <h3 style="font-size:15px;color:var(--text)">${item ? "编辑视频" : "新增视频"}</h3>
    <div class="drawer-field">
      <label>标题</label>
      <input type="text" data-f="title" value="${esc(item?.title ?? "")}" placeholder="标题" />
    </div>
    <div class="drawer-field">
      <label>分类</label>
      <select data-f="category">
        <option value="电影"${category === "电影" ? " selected" : ""}>电影</option>
        <option value="动漫"${category === "动漫" ? " selected" : ""}>动漫</option>
        <option value="剧集"${category === "剧集" ? " selected" : ""}>剧集</option>
      </select>
    </div>
    <div class="drawer-field">
      <label>分类路径</label>
      <input type="text" data-f="categoryPath" value="${esc(item?.category_path ?? "")}" placeholder="如 电影/科幻/星战" />
    </div>
    <div class="drawer-field">
      <label>视频文件</label>
      <div style="font-size:12px;color:var(--text-dim);word-break:break-all" data-d="path"></div>
      <button type="button" data-b="path">选择视频</button>
    </div>
    <div class="drawer-field">
      <label>字幕</label>
      <div style="font-size:12px;color:var(--text-dim);word-break:break-all" data-d="subtitle"></div>
      <button type="button" data-b="subtitle">选择字幕</button>
    </div>
    <div class="drawer-field">
      <label>展示图</label>
      <img class="drawer-cover-preview" data-d="cover" alt="" />
      <button type="button" data-b="cover">选择图片</button>
    </div>
    <div class="drawer-field">
      <label>简介</label>
      <textarea data-f="description" rows="4" placeholder="简介">${esc(item?.description ?? "")}</textarea>
    </div>
    <div class="drawer-actions">
      <button type="button" data-b="save">保存</button>
      <button type="button" data-b="cancel">取消</button>
    </div>
  `;

  const q = <T extends HTMLElement>(sel: string): T => drawer.querySelector(sel) as T;
  const pathDisp = q<HTMLDivElement>('[data-d="path"]');
  const subDisp = q<HTMLDivElement>('[data-d="subtitle"]');
  const coverImg = q<HTMLImageElement>('[data-d="cover"]');

  const refreshPath = () => { pathDisp.textContent = path && !path.startsWith("pending:") ? path : "（未选择）"; };
  const refreshSub = () => { subDisp.textContent = subtitlePath || "（未选择）"; };
  const refreshCover = () => {
    if (coverPath) {
      coverImg.src = convertFileSrc(coverPath);
      coverImg.style.display = "";
    } else {
      coverImg.removeAttribute("src");
      coverImg.style.display = "none";
    }
  };
  refreshPath();
  refreshSub();
  refreshCover();

  // 关闭机制
  const onKey = (e: KeyboardEvent) => { if (e.key === "Escape") close(); };
  const close = () => {
    overlay.remove();
    drawer.remove();
    document.removeEventListener("keydown", onKey, true);
  };
  overlay.onclick = () => close();
  document.addEventListener("keydown", onKey, true);

  // 视频文件选择
  q<HTMLButtonElement>('[data-b="path"]').onclick = async () => {
    const f = await open({ multiple: false, filters: [{ name: "视频", extensions: ["mkv", "mp4"] }] });
    if (typeof f === "string") { path = f; refreshPath(); }
  };

  // 字幕选择
  q<HTMLButtonElement>('[data-b="subtitle"]').onclick = async () => {
    const f = await open({ multiple: false, filters: [{ name: "字幕", extensions: ["ass"] }] });
    if (typeof f === "string") { subtitlePath = f; refreshSub(); }
  };

  // 展示图选择 → 拷贝 → 预览
  q<HTMLButtonElement>('[data-b="cover"]').onclick = async () => {
    const f = await open({ multiple: false, filters: [{ name: "图片", extensions: ["jpg", "jpeg", "png", "webp"] }] });
    if (typeof f === "string") {
      coverPath = await api.importCover(f);
      refreshCover();
    }
  };

  // 保存
  q<HTMLButtonElement>('[data-b="save"]').onclick = async () => {
    const title = q<HTMLInputElement>('[data-f="title"]').value.trim();
    const cat = q<HTMLSelectElement>('[data-f="category"]').value;
    const catPath = q<HTMLInputElement>('[data-f="categoryPath"]').value.trim();
    const desc = q<HTMLTextAreaElement>('[data-f="description"]').value.trim();
    // 新增时空 path 用唯一占位，避免 media_item.path UNIQUE 冲突
    const finalPath = path || `pending:${Date.now()}`;
    try {
      if (item?.id) {
        await api.mediaUpdate(item.id, cat, catPath, title, finalPath,
          subtitlePath || null, coverPath || null, desc || null);
      } else {
        await api.mediaCreate(cat, catPath, title, finalPath,
          subtitlePath || null, coverPath || null, desc || null);
      }
      close();
      onSaved();
    } catch (err) {
      alert("保存失败：" + String(err));
    }
  };

  // 取消
  q<HTMLButtonElement>('[data-b="cancel"]').onclick = () => close();

  document.body.appendChild(overlay);
  document.body.appendChild(drawer);
}
