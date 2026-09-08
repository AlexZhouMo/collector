import { convertFileSrc } from "@tauri-apps/api/core";
import type { MediaItem } from "../lib/ipc";
import type { TreeNode } from "../lib/videoTree";
import { findNode } from "../lib/videoTree";
import { esc } from "../lib/escape";

/**
 * 树视图：左侧可展开目录树，右侧显示选中节点的视频海报。
 */
export function TreeView(
  root: TreeNode,
  onOpen: (it: MediaItem) => void,
  onContext?: (it: MediaItem, x: number, y: number) => void
): HTMLElement {
  const el = document.createElement("div");
  el.className = "tree-view";
  const expanded = new Set<string>([root.path]);
  let selected = root.path;

  const renderTreeNodes = (node: TreeNode, depth: number): string => {
    const isOpen = expanded.has(node.path);
    const hasChildren = node.children.length > 0;
    const arrow = hasChildren ? (isOpen ? "▾" : "▸") : "　";
    const row = `<div class="tree-node ${selected === node.path ? "sel" : ""}" data-path="${esc(node.path)}" style="padding-left:${depth * 14 + 4}px">
      <span class="tree-arrow" data-toggle="${esc(node.path)}">${arrow}</span>
      <span class="tree-name">${esc(node.name)}</span>
    </div>`;
    const childrenHtml = isOpen ? node.children.map(c => renderTreeNodes(c, depth + 1)).join("") : "";
    return row + childrenHtml;
  };

  const render = () => {
    const node = findNode(root, selected) ?? root;
    const posters = node.items.map((it, i) => `
      <div class="poster card-hover" data-i="${i}">
        <div class="poster-img">${it.cover_path ? `<img src="${convertFileSrc(it.cover_path)}"/>` : `<div class="poster-ph">${esc(it.title)}</div>`}</div>
        <div class="poster-title">${esc(it.title)}</div>
      </div>`).join("");
    el.innerHTML = `
      <div class="tree-pane">${renderTreeNodes(root, 0)}</div>
      <div class="tree-content"><div class="poster-grid">${posters || '<div class="tree-empty">此目录下无直接视频，请展开子目录</div>'}</div></div>`;

    el.querySelectorAll<HTMLElement>(".tree-arrow").forEach(a =>
      a.onclick = (e) => { e.stopPropagation(); const p = a.dataset.toggle!; expanded.has(p) ? expanded.delete(p) : expanded.add(p); render(); });
    el.querySelectorAll<HTMLElement>(".tree-node").forEach(n =>
      n.onclick = () => { selected = n.dataset.path!; render(); });
    el.querySelectorAll<HTMLElement>(".tree-content .poster").forEach(p => {
      p.onclick = () => onOpen(node.items[Number(p.dataset.i)]);
      p.oncontextmenu = (e) => { e.preventDefault(); onContext?.(node.items[Number(p.dataset.i)], e.clientX, e.clientY); };
    });
  };
  render();
  return el;
}
