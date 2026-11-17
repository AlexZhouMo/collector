import { convertFileSrc } from "@tauri-apps/api/core";
import type { MediaItem } from "../lib/ipc";
import type { TreeNode } from "../lib/videoTree";
import { findNode } from "../lib/videoTree";
import { esc } from "../lib/escape";
import { displayTitle } from "../lib/displayTitle";
import { icon } from "../lib/icons";
import { FOLDER_SVG } from "./FolderView";
import { nameCollator } from "../lib/displayPath";

/**
 * 树视图：左侧可展开目录树，右侧显示选中节点的视频海报。
 * initialSelected: 初始选中节点（刷新后保留位置用）；onNav: 选中变化回调。
 */
export function TreeView(
  root: TreeNode,
  onOpen: (it: MediaItem) => void,
  onContext?: (it: MediaItem, x: number, y: number) => void,
  initialSelected?: string,
  onNav?: (path: string) => void,
  kind: string = "video"
): HTMLElement {
  const el = document.createElement("div");
  el.className = "tree-view";
  const expanded = new Set<string>([root.path]);
  // 初始选中：优先 initialSelected，需在树中存在，否则回退根
  let selected = initialSelected && findNode(root, initialSelected) ? initialSelected : root.path;
  // 展开选中节点的祖先链，使刷新后能看到它
  if (selected !== root.path) {
    const segs = selected.split("/");
    for (let i = 1; i <= segs.length; i++) expanded.add(segs.slice(0, i).join("/"));
  }

  const renderTreeNodes = (node: TreeNode, depth: number): string => {
    const isOpen = expanded.has(node.path);
    const hasChildren = node.children.length > 0;
    const arrow = hasChildren ? icon("chevronRight", 12) : "";
    const row = `<div class="tree-node ${selected === node.path ? "sel" : ""}" data-path="${esc(node.path)}" style="padding-left:${depth * 14 + 4}px">
      <span class="tree-arrow${isOpen ? " open" : ""}"${hasChildren ? ` data-toggle="${esc(node.path)}"` : ""}>${arrow}</span>
      <span class="tree-name">${esc(node.name)}</span>
    </div>`;
    const childrenHtml = isOpen ? node.children.map(c => renderTreeNodes(c, depth + 1)).join("") : "";
    return row + childrenHtml;
  };

  const toggle = (p: string) => { expanded.has(p) ? expanded.delete(p) : expanded.add(p); };
  // 双击标题展开/折叠。单击会重建 DOM，原生 dblclick 的目标元素已被替换、不可靠，故按同一节点两次点击的间隔判断。
  let lastClick = { path: "", at: 0 };

  const render = () => {
    const node = findNode(root, selected) ?? root;
    // 重建前记住左侧树的滚动位置，否则每次展开/选中都会跳回顶部
    const prevScroll = el.querySelector<HTMLElement>(".tree-pane")?.scrollTop ?? 0;
    // 漫画：作品条目用文件夹图标，封面只在「卷」一级显示
    const asFolder = kind === "comic";
    const order = node.items.map((it, i) => ({ it, i }));
    if (asFolder) order.sort((a, b) => nameCollator.compare(displayTitle(a.it.title), displayTitle(b.it.title)));
    const posters = order.map(({ it, i }) => asFolder ? `
      <div class="poster fv-folder${it.playable === false ? " disabled" : ""}" data-i="${i}">
        <div class="fv-folder-icon">${FOLDER_SVG}</div>
        <div class="poster-title">${esc(displayTitle(it.title))}</div>
      </div>` : `
      <div class="poster${it.playable === false ? " disabled" : ""}" data-i="${i}">
        <div class="poster-img">${it.cover_path ? `<img src="${convertFileSrc(it.cover_path)}"/>` : `<div class="poster-ph">${esc(displayTitle(it.title))}</div>`}</div>
        <div class="poster-title">${esc(displayTitle(it.title))}</div>
      </div>`).join("");
    el.innerHTML = `
      <div class="tree-pane">${renderTreeNodes(root, 0)}</div>
      <div class="tree-content"><div class="poster-grid">${posters || '<div class="tree-empty">此目录下无直接视频，请展开子目录</div>'}</div></div>`;

    el.querySelector<HTMLElement>(".tree-pane")!.scrollTop = prevScroll;

    el.querySelectorAll<HTMLElement>(".tree-arrow[data-toggle]").forEach(a =>
      a.onclick = (e) => { e.stopPropagation(); toggle(a.dataset.toggle!); lastClick = { path: "", at: 0 }; render(); });
    el.querySelectorAll<HTMLElement>(".tree-node").forEach(n =>
      n.onclick = () => {
        const p = n.dataset.path!;
        const now = Date.now();
        if (lastClick.path === p && now - lastClick.at < 400) {
          if (findNode(root, p)?.children.length) toggle(p);
          lastClick = { path: "", at: 0 };
        } else {
          lastClick = { path: p, at: now };
        }
        selected = p; onNav?.(selected); render();
      });
    el.querySelectorAll<HTMLElement>(".tree-content .poster").forEach(p => {
      p.onclick = () => { const it = node.items[Number(p.dataset.i)]; if (it.playable === false) return; onOpen(it); };
      p.oncontextmenu = (e) => { e.preventDefault(); onContext?.(node.items[Number(p.dataset.i)], e.clientX, e.clientY); };
    });
  };
  render();
  return el;
}
