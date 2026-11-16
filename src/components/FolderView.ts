import { convertFileSrc } from "@tauri-apps/api/core";
import type { MediaItem } from "../lib/ipc";
import type { TreeNode } from "../lib/videoTree";
import { findNode } from "../lib/videoTree";
import { esc } from "../lib/escape";
import { displayTitle } from "../lib/displayTitle";
import { attachInlineRename } from "./InlineRename";
import { showContextMenu } from "./ContextMenu";
import { api } from "../lib/ipc";
import { showToast } from "./Toast";

/**
 * 大号文件夹图形（类似系统资源管理器的"大图标"）：后板+标签页、露出的纸张、前盖三层。
 * 渐变 id 在每个图标里重复定义，内容完全相同，浏览器取任一份结果都一样。
 */
const FOLDER_SVG = `<svg class="fv-folder-svg" viewBox="0 0 52 50" aria-hidden="true">
  <defs>
    <linearGradient id="fvg-back" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="#4d74d6"/><stop offset="1" stop-color="#2d4ea6"/></linearGradient>
    <linearGradient id="fvg-front" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="#86adff"/><stop offset="1" stop-color="#4f7ff0"/></linearGradient>
  </defs>
  <path d="M4 6a3 3 0 0 1 3-3h12.3a3 3 0 0 1 2.1.9L24.6 7H45a3 3 0 0 1 3 3v35a3 3 0 0 1-3 3H7a3 3 0 0 1-3-3Z" fill="url(#fvg-back)"/>
  <rect x="8" y="11" width="36" height="24" rx="1.5" fill="#e9efff" opacity=".9"/>
  <path d="M2.2 18.4A3.2 3.2 0 0 1 5.4 15h41.2a3.2 3.2 0 0 1 3.2 3.4l-2.4 26.4A3.5 3.5 0 0 1 43.9 48H8.1a3.5 3.5 0 0 1-3.5-3.2Z" fill="url(#fvg-front)"/>
  <path d="M5.4 15.8h41.2" stroke="#fff" stroke-opacity=".35" stroke-width="1" stroke-linecap="round"/>
</svg>`;

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
  enableRename: boolean = true,
  kind: string = "video",
  onFolderMove?: (folderPath: string, folderName: string) => void
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

    // 文件区标题随类型：影视=视频 / 漫画=漫画 / 游戏=游戏
    const fileLabel = kind === "comic" ? "漫画" : kind === "game" ? "游戏" : "视频";

    // 文件夹统一大号文件夹图形（含 comic/game 各级），无方框，悬停时整格高亮 + 图标发光。
    const folders = node.children.map(c => `
      <div class="fv-cell fv-folder" data-folder="${esc(c.path)}">
        <div class="fv-folder-icon">${FOLDER_SVG}</div>
        <span class="fv-name fv-name-editable" data-folder-name="${esc(c.path)}">${esc(c.name)}</span>
      </div>`).join("");
    const videos = node.items.map((it, i) => `
      <div class="fv-cell fv-video${it.playable === false ? " disabled" : ""}" data-i="${i}">
        <div class="poster-img">${it.cover_path ? `<img src="${convertFileSrc(it.cover_path)}"/>` : `<div class="poster-ph">${esc(displayTitle(it.title))}</div>`}</div>
        <span class="fv-name">${esc(displayTitle(it.title))}</span>
      </div>`).join("");

    const folderCount = node.children.length;
    const fileCount = node.items.length;
    const bothPresent = folderCount > 0 && fileCount > 0;

    const folderSection = folderCount > 0 ? `
      <div class="fv-section">
        <div class="fv-sec-label">目录 <span class="fv-sec-count">${folderCount}</span></div>
        <div class="fv-grid">${folders}</div>
      </div>` : "";
    const fileSection = fileCount > 0 ? `
      <div class="fv-section">
        <div class="fv-sec-label">${fileLabel} <span class="fv-sec-count">${fileCount}</span></div>
        <div class="fv-grid">${videos}</div>
      </div>` : "";
    const divider = bothPresent ? `<div class="fv-divider"></div>` : "";

    el.innerHTML = `
      <div class="breadcrumb">${crumbs}</div>
      ${folderSection}${divider}${fileSection}`;

    el.querySelectorAll<HTMLElement>(".crumb").forEach(c =>
      c.onclick = () => go(c.dataset.path!));
    el.querySelectorAll<HTMLElement>(".fv-folder").forEach(f => {
      f.onclick = () => go(f.dataset.folder!);
      // 一级目录（分类根下第一层，path 不含 "/"）是固定题材分区，禁止移动；仅二级及更深可移。
      if (onFolderMove) {
        f.oncontextmenu = (e) => {
          e.preventDefault();
          const path = f.dataset.folder!;
          if (!path.includes("/")) return; // 一级目录不弹移动菜单
          const child = node.children.find(c => c.path === path);
          if (!child) return;
          showContextMenu(e.clientX, e.clientY, [
            { label: "移动", onClick: () => onFolderMove(child.path, child.name) },
          ]);
        };
      }
    });
    if (enableRename) {
      el.querySelectorAll<HTMLElement>(".fv-name-editable").forEach((nameEl) => {
        const path = nameEl.dataset.folderName!;
        const child = node.children.find((c) => c.path === path);
        if (!child) return;
        attachInlineRename(nameEl, child.name, async (newName) => {
          try {
            await api.renameFolder(kind, root.name, child.path, newName);
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
