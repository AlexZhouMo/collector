import type { MediaItem } from "../lib/ipc";
import { api } from "../lib/ipc";
import type { TreeNode } from "../lib/videoTree";
import { findNode } from "../lib/videoTree";
import { matchTree } from "../lib/moveTreeFilter";
import { showToast } from "./Toast";
import { esc } from "../lib/escape";
import { displayTitle } from "../lib/displayTitle";

/**
 * 打开「移动到目标文件夹」模态弹窗。
 * item: 被移动视频；tree: 当前分类完整目录树（根 path=""）；
 * onMoved: 移动成功后回调（通常为 VideoView 的 refresh）。
 */
export function openMoveDialog(
  item: MediaItem,
  tree: TreeNode,
  kind: string,
  onMoved: () => void
): void {
  const expanded = new Set<string>([tree.path]);
  let query = "";
  let selected: string | null = null;

  const overlay = document.createElement("div");
  overlay.className = "modal-overlay";
  const dialog = document.createElement("div");
  dialog.className = "move-dialog glass";
  overlay.appendChild(dialog);

  const close = () => {
    overlay.remove();
    document.removeEventListener("keydown", onKey, true);
  };
  const onKey = (e: KeyboardEvent) => { if (e.key === "Escape") close(); };

  const targetName = (): string => {
    if (selected === null) return "";
    if (selected === tree.path) return tree.name;
    const n = findNode(tree, selected);
    return n ? n.name : selected;
  };

  const render = () => {
    const { visible, forceExpand } = matchTree(tree, query);

    const renderNode = (node: TreeNode, depth: number): string => {
      if (!visible.has(node.path)) return "";
      const isRoot = node.path === tree.path;
      const isCur = node.path === item.category_path;
      const hasChildren = node.children.length > 0;
      const isOpen = query ? forceExpand.has(node.path) : expanded.has(node.path);
      const arrow = hasChildren ? (isOpen ? "▾" : "▸") : "　";
      const name = isRoot ? `${node.name}（根）` : node.name;
      const cls = "move-node" + (selected === node.path ? " sel" : "") + (isCur ? " cur" : "");
      const tag = isCur ? `<span class="move-tag">当前位置</span>` : "";
      const row = `<div class="${cls}" data-path="${esc(node.path)}" style="padding-left:${depth * 14 + 4}px">
        <span class="move-arrow" data-toggle="${esc(node.path)}">${arrow}</span>
        <span class="move-name">${esc(name)}</span>${tag}
      </div>`;
      const childrenHtml = isOpen ? node.children.map(c => renderNode(c, depth + 1)).join("") : "";
      return row + childrenHtml;
    };

    const canMove = selected !== null;
    const moveLabel = canMove ? `移动到「${esc(targetName())}」` : "请选择目标文件夹";

    dialog.innerHTML = `
      <div class="move-title">移动 <b>「${esc(displayTitle(item.title))}」</b> 到…</div>
      <input class="move-search" placeholder="🔍 搜索文件夹" value="${esc(query)}"/>
      <div class="move-tree">${renderNode(tree, 0)}</div>
      <div class="move-hint">搜索缩小范围；点箭头展开/折叠；点名称选中目标</div>
      <div class="move-actions">
        <button class="btn move-cancel">取消</button>
        <button class="btn btn-primary move-confirm${canMove ? "" : " disabled"}">${moveLabel}</button>
      </div>`;

    const search = dialog.querySelector<HTMLInputElement>(".move-search")!;
    search.oninput = () => {
      query = search.value;
      render();
      const next = dialog.querySelector<HTMLInputElement>(".move-search");
      if (next) {
        next.focus();
        const len = next.value.length;
        next.setSelectionRange(len, len);
      }
    };

    dialog.querySelectorAll<HTMLElement>(".move-arrow").forEach(a =>
      a.onclick = (e) => {
        e.stopPropagation();
        const p = a.dataset.toggle!;
        if (query) return;
        expanded.has(p) ? expanded.delete(p) : expanded.add(p);
        render();
      });

    dialog.querySelectorAll<HTMLElement>(".move-node").forEach(n =>
      n.onclick = () => {
        const p = n.dataset.path!;
        if (p === item.category_path) return;
        selected = p;
        render();
      });

    dialog.querySelector<HTMLElement>(".move-cancel")!.onclick = close;
    dialog.querySelector<HTMLElement>(".move-confirm")!.onclick = async () => {
      if (selected === null) return;
      const name = targetName();
      try {
        if (kind === "game") {
          await api.gameUpdate(item.id, selected, item.title,
            item.cover_path || null, item.description || null);
        } else {
          await api.mediaUpdate(item.id, item.category, selected, item.title,
            item.cover_path, item.description);
        }
        close();
        showToast(`已移动到「${name}」`);
        onMoved();
      } catch (e) {
        showToast("移动失败：" + e, "error");
      }
    };
  };

  overlay.onclick = (e) => { if (e.target === overlay) close(); };
  document.addEventListener("keydown", onKey, true);
  render();
  document.body.appendChild(overlay);
  dialog.querySelector<HTMLInputElement>(".move-search")?.focus();
}
