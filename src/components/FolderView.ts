import { convertFileSrc } from "@tauri-apps/api/core";
import type { MediaItem } from "../lib/ipc";
import type { TreeNode } from "../lib/videoTree";
import { findNode } from "../lib/videoTree";
import { icon } from "../lib/icons";
import { esc } from "../lib/escape";

/**
 * 文件夹视图：进入式浏览一棵 TreeNode。
 * root: 分类树根；onOpen: 打开视频。内部维护当前路径。
 */
export function FolderView(
  root: TreeNode,
  onOpen: (it: MediaItem) => void,
  onContext?: (it: MediaItem, x: number, y: number) => void
): HTMLElement {
  const el = document.createElement("div");
  el.className = "folder-view";
  let currentPath = root.path;

  const render = () => {
    const node = findNode(root, currentPath) ?? root;
    const segs = currentPath.split("/");
    const crumbs = segs.map((seg, i) => {
      const p = segs.slice(0, i + 1).join("/");
      const last = i === segs.length - 1;
      return `<span class="crumb ${last ? "crumb-cur" : ""}" data-path="${esc(p)}">${esc(seg)}</span>`;
    }).join('<span class="crumb-sep">／</span>');

    const folders = node.children.map(c => `
      <div class="fv-cell fv-folder" data-folder="${esc(c.path)}">
        <div class="fv-folder-icon">${icon("folder", 40)}</div>
        <span class="fv-name">${esc(c.name)}</span>
      </div>`).join("");
    const videos = node.items.map((it, i) => `
      <div class="fv-cell fv-video" data-i="${i}">
        <div class="poster-img">${it.cover_path ? `<img src="${convertFileSrc(it.cover_path)}"/>` : `<div class="poster-ph">${esc(it.title)}</div>`}</div>
        <span class="fv-name">${esc(it.title)}</span>
      </div>`).join("");

    el.innerHTML = `
      <div class="breadcrumb">${crumbs}</div>
      <div class="fv-grid">${folders}${videos}</div>`;

    el.querySelectorAll<HTMLElement>(".crumb").forEach(c =>
      c.onclick = () => { currentPath = c.dataset.path!; render(); });
    el.querySelectorAll<HTMLElement>(".fv-folder").forEach(f =>
      f.onclick = () => { currentPath = f.dataset.folder!; render(); });
    el.querySelectorAll<HTMLElement>(".fv-video").forEach(v => {
      v.onclick = () => onOpen(node.items[Number(v.dataset.i)]);
      v.oncontextmenu = (e) => { e.preventDefault(); onContext?.(node.items[Number(v.dataset.i)], e.clientX, e.clientY); };
    });
  };
  render();
  return el;
}
