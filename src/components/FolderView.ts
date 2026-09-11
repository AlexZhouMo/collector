import { convertFileSrc } from "@tauri-apps/api/core";
import type { MediaItem } from "../lib/ipc";
import type { TreeNode } from "../lib/videoTree";
import { findNode } from "../lib/videoTree";
import { icon } from "../lib/icons";
import { esc } from "../lib/escape";
import { displayTitle } from "../lib/displayTitle";
import { attachInlineRename } from "./InlineRename";
import { api } from "../lib/ipc";
import { showToast } from "./Toast";

/**
 * 文件夹视图：进入式浏览一棵 TreeNode。
 * root: 分类树根；onOpen: 打开视频；initialPath: 初始所在文件夹（刷新后保留位置用）；
 * onNav: 当前路径变化回调（同步给上层保存位置）。
 */
export function FolderView(
  root: TreeNode,
  onOpen: (it: MediaItem) => void,
  onContext?: (it: MediaItem, x: number, y: number) => void,
  initialPath?: string,
  onNav?: (path: string) => void,
  onRenamed?: (oldPath: string, newPath: string) => void,
  enableRename: boolean = true
): HTMLElement {
  const el = document.createElement("div");
  el.className = "folder-view";
  // 初始路径：优先用 initialPath，但需在当前树中存在（移动/删除后可能已失效），否则回退根
  let currentPath = initialPath && findNode(root, initialPath) ? initialPath : root.path;

  const go = (path: string) => { currentPath = path; onNav?.(path); render(); };

  const render = () => {
    const node = findNode(root, currentPath) ?? root;
    // 面包屑：首级固定为分类名（root.name，指向分类根=空串路径），
    // 其后接 currentPath 分类内相对各段，逐段可点回退。如 电影 / 谍战。
    const inner = currentPath ? currentPath.split("/").filter(Boolean) : [];
    const crumbNodes = [{ name: root.name, path: "" }];
    let acc = "";
    for (const seg of inner) {
      acc = acc ? `${acc}/${seg}` : seg;
      crumbNodes.push({ name: seg, path: acc });
    }
    const crumbs = crumbNodes.map((c, i) => {
      const last = i === crumbNodes.length - 1;
      return `<span class="crumb ${last ? "crumb-cur" : ""}" data-path="${esc(c.path)}">${esc(c.name)}</span>`;
    }).join('<span class="crumb-sep">／</span>');

    const folders = node.children.map(c => `
      <div class="fv-cell fv-folder" data-folder="${esc(c.path)}">
        <div class="fv-folder-icon">${icon("folder", 72)}</div>
        <span class="fv-name fv-name-editable" data-folder-name="${esc(c.path)}">${esc(c.name)}</span>
      </div>`).join("");
    const videos = node.items.map((it, i) => `
      <div class="fv-cell fv-video${it.playable === false ? " disabled" : ""}" data-i="${i}">
        <div class="poster-img">${it.cover_path ? `<img src="${convertFileSrc(it.cover_path)}"/>` : `<div class="poster-ph">${esc(displayTitle(it.title))}</div>`}</div>
        <span class="fv-name">${esc(displayTitle(it.title))}</span>
      </div>`).join("");

    el.innerHTML = `
      <div class="breadcrumb">${crumbs}</div>
      <div class="fv-grid">${folders}${videos}</div>`;

    el.querySelectorAll<HTMLElement>(".crumb").forEach(c =>
      c.onclick = () => go(c.dataset.path!));
    el.querySelectorAll<HTMLElement>(".fv-folder").forEach(f =>
      f.onclick = () => go(f.dataset.folder!));
    if (enableRename) {
      el.querySelectorAll<HTMLElement>(".fv-name-editable").forEach((nameEl) => {
        const path = nameEl.dataset.folderName!;
        const child = node.children.find((c) => c.path === path);
        if (!child) return;
        attachInlineRename(nameEl, child.name, async (newName) => {
          try {
            await api.renameFolder(root.name, child.path, newName);
          } catch (e) {
            showToast("重命名失败：" + e, "error");
            throw e; // 让 InlineRename 保持编辑态
          }
          showToast("已重命名");
          const parent = child.path.includes("/") ? child.path.slice(0, child.path.lastIndexOf("/")) : "";
          const newPath = parent ? `${parent}/${newName}` : newName;
          onRenamed?.(child.path, newPath);
        });
      });
    }
    el.querySelectorAll<HTMLElement>(".fv-video").forEach(v => {
      v.onclick = () => { const it = node.items[Number(v.dataset.i)]; if (it.playable === false) return; onOpen(it); };
      v.oncontextmenu = (e) => { e.preventDefault(); onContext?.(node.items[Number(v.dataset.i)], e.clientX, e.clientY); };
    });
  };
  render();
  return el;
}
