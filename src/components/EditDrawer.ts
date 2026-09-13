import { api } from "../lib/ipc";
import type { MediaItem } from "../lib/ipc";
import { open } from "@tauri-apps/plugin-dialog";
import { convertFileSrc } from "@tauri-apps/api/core";
import { openCoverCropper } from "./CoverCropper";
import { esc } from "../lib/escape";
import { icon } from "../lib/icons";

/// 打开右侧滑入抽屉表单，用于新增(item=null)或编辑(item 有值)视频条目。
/// 保存成功后关闭抽屉并回调 onSaved()。取消/遮罩点击/Esc 关闭不保存。
/// defaultCategory：新增时落入的分类（分类与分类路径不再在表单里编辑，
/// 编辑保留原值，新增用当前分类作为分类与分类路径）。
export function openEditDrawer(item: MediaItem | null, onSaved: () => void, defaultCategory = "电影", kind: "video" | "comic" | "game" = "video"): void {
  // 局部维护的可变字段（文件选择后更新）
  let coverPath = item?.cover_path ?? "";

  // 分类与分类路径不在表单里编辑：编辑保留原值，新增落入当前分类根。
  const category = item?.category ?? defaultCategory;
  const categoryPath = item?.category_path ?? defaultCategory;

  const overlay = document.createElement("div");
  overlay.className = "drawer-overlay";

  const drawer = document.createElement("div");
  drawer.className = "edit-drawer glass";

  drawer.innerHTML = `
    <h3 style="font-size:15px;color:var(--text)">${kind === "comic" ? (item ? "编辑漫画" : "新增漫画") : kind === "game" ? (item ? "编辑游戏" : "新增游戏") : (item ? "编辑视频" : "新增视频")}</h3>
    <div class="drawer-field">
      <label>标题</label>
      <input type="text" data-f="title" value="${esc(item?.title ?? "")}" placeholder="标题" />
    </div>
    ${kind !== "video" ? "" : `<div class="drawer-field">
      <label>视频路径</label>
      <div style="font-size:12px;color:var(--text-dim);word-break:break-all">${esc(item?.video_path || "（未定位到视频文件）")}</div>
    </div>`}
    <div class="drawer-field">
      <label>封面</label>
      <div class="cover-box" data-d="coverbox">
        <img class="drawer-cover-preview" data-d="cover" alt="" />
        <button type="button" class="cover-del" data-b="cover-del" title="删除封面">${icon("trash", 15)}</button>
      </div>
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
  const coverImg = q<HTMLImageElement>('[data-d="cover"]');
  const coverBox = q<HTMLDivElement>('[data-d="coverbox"]');

  // 记录进入编辑时的原封面路径，保存时据此删旧文件
  const originalCover = item?.cover_path ?? "";

  const refreshCover = () => {
    if (coverPath) {
      coverImg.src = convertFileSrc(coverPath);
      coverBox.style.display = "";
    } else {
      coverImg.removeAttribute("src");
      coverBox.style.display = "none"; // 无封面隐藏整个容器（含删除图标）
    }
  };
  // 删除封面：仅清本地预览，保存时才真正删库+删文件
  q<HTMLButtonElement>('[data-b="cover-del"]').onclick = () => { coverPath = ""; refreshCover(); };
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


  // 封面选择 → 裁剪弹窗 → 后端生成标准海报 → 预览
  q<HTMLButtonElement>('[data-b="cover"]').onclick = async () => {
    const f = await open({ multiple: false, filters: [{ name: "图片", extensions: ["jpg", "jpeg", "png", "webp"] }] });
    if (typeof f === "string") {
      openCoverCropper(f, (p) => { coverPath = p; refreshCover(); }, kind);
    }
  };

  // 保存
  q<HTMLButtonElement>('[data-b="save"]').onclick = async () => {
    const title = q<HTMLInputElement>('[data-f="title"]').value.trim();
    const desc = q<HTMLTextAreaElement>('[data-f="description"]').value.trim();
    try {
      if (kind === "comic") {
        // 漫画只编辑现有条目（item 必有值），保留原分类路径
        if (item?.id) {
          await api.comicUpdate(item.id, categoryPath, title,
            coverPath || null, desc || null);
        }
      } else if (kind === "game") {
        // 游戏只编辑现有条目（item 必有值），保留原分类路径
        if (item?.id) {
          await api.gameUpdate(item.id, categoryPath, title,
            coverPath || null, desc || null);
        }
      } else if (item?.id) {
        await api.mediaUpdate(item.id, category, categoryPath, title,
          coverPath || null, desc || null);
      } else {
        await api.mediaCreate(category, categoryPath, title,
          coverPath || null, desc || null);
      }
      // 原封面被删或被换新图 → 删除旧磁盘文件（失败忽略，不阻断保存）
      if (originalCover && originalCover !== coverPath) {
        try { await api.deleteCoverFile(originalCover); } catch { /* 忽略 */ }
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
