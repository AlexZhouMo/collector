import type { MediaItem } from "../lib/ipc";
import { api } from "../lib/ipc";
import type { TreeNode } from "../lib/videoTree";
import { findNode, splitCategoryPath } from "../lib/videoTree";
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
  onMoved: () => void,
  crossCategory: boolean = false
): void {
  const expanded = new Set<string>([tree.path]);
  let query = "";
  let selected: string | null = null;

  // crossCategory 下当前位置为带分类前缀的 path。
  const currentPath = crossCategory
    ? item.category + (item.category_path ? "/" + item.category_path : "")
    : item.category_path;
  const isVirtualRoot = (p: string) => crossCategory && p === "__root__";
  const isCurrent = (p: string) => p === currentPath;

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
      const isCur = isCurrent(node.path);
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
        if (isVirtualRoot(p) || isCurrent(p)) return;
        selected = p;
        render();
      });

    dialog.querySelector<HTMLElement>(".move-cancel")!.onclick = close;
    dialog.querySelector<HTMLElement>(".move-confirm")!.onclick = async () => {
      if (selected === null) return;
      const name = targetName();
      try {
        if (crossCategory) {
          const { category: targetCategory, categoryPath: targetCategoryPath } =
            splitCategoryPath(selected);
          await api.mediaUpdate(item.id, targetCategory, targetCategoryPath,
            item.title, item.cover_path, item.description);
        } else if (kind === "game") {
          await api.gameUpdate(item.id, selected, item.title,
            item.cover_path || null, item.description || null);
        } else if (kind === "comic") {
          await api.comicUpdate(item.id, selected, item.title,
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

/**
 * 打开「移动文件夹」模态弹窗。
 * folderName: 被移动文件夹名（用于标题）；
 * folderPath: 源文件夹在其分类内的相对路径（不含分类前缀，如 "科幻/诺兰"）；
 * sourceCategory: 源文件夹所属分类（传后端 category）；
 * tree: 目标树。影视传 buildMergedVideoTree 合一树（crossCategory=true，节点 path 带分类前缀）；
 *       漫画/游戏传单分类树（crossCategory=false，节点 path 不含分类前缀）；
 * onMoved: 移动成功回调（refresh）。
 *
 * 禁选：虚拟根（path="__root__"）；被移动文件夹自身及其子树（按带/不带前缀的源 path 比对）。
 * 后端 old_path 不含分类名，故传参用 folderPath（不含前缀）+ sourceCategory。
 * 影视 target：selected（带前缀）→ splitCategoryPath → {targetCategory, targetParent=categoryPath}。
 * 漫画/游戏 target：targetCategory=sourceCategory（固定分类），targetParent=selected（不含前缀）。
 */
export function openFolderMoveDialog(opts: {
  folderName: string;
  folderPath: string;
  sourceCategory: string;
  tree: TreeNode;
  kind: string;
  crossCategory: boolean;
  onMoved: () => void;
}): void {
  const { folderName, folderPath, sourceCategory, tree, kind, crossCategory, onMoved } = opts;
  const expanded = new Set<string>([tree.path]);
  let query = "";
  let selected: string | null = null;

  // 源文件夹在当前树里的 path（用于禁选自身子树）：
  // 影视合一树带分类前缀 → sourceCategory[/folderPath]；单分类树不含前缀 → folderPath。
  const sourcePathInTree = crossCategory
    ? (folderPath ? `${sourceCategory}/${folderPath}` : sourceCategory)
    : folderPath;

  const isVirtualRoot = (p: string) => crossCategory && p === "__root__";
  // 自身或子树：目标 path == 源 path，或以 源path + "/" 开头。
  const isSelfOrDesc = (p: string) => p === sourcePathInTree || p.startsWith(sourcePathInTree + "/");
  const isDisabled = (p: string) => isVirtualRoot(p) || isSelfOrDesc(p);

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
      const disabled = isDisabled(node.path);
      const hasChildren = node.children.length > 0;
      const isOpen = query ? forceExpand.has(node.path) : expanded.has(node.path);
      const arrow = hasChildren ? (isOpen ? "▾" : "▸") : "　";
      const name = isRoot ? `${node.name}（根）` : node.name;
      const cls = "move-node" + (selected === node.path ? " sel" : "") + (disabled ? " cur disabled" : "");
      const tag = node.path === sourcePathInTree ? `<span class="move-tag">当前文件夹</span>` : "";
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
      <div class="move-title">移动文件夹 <b>「${esc(folderName)}」</b> 到…</div>
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
        if (isDisabled(p)) return;
        selected = p;
        render();
      });

    dialog.querySelector<HTMLElement>(".move-cancel")!.onclick = close;
    dialog.querySelector<HTMLElement>(".move-confirm")!.onclick = async () => {
      if (selected === null) return;
      const name = targetName();
      // 目标分类与父目录（不含前缀）。
      let targetCategory: string;
      let targetParent: string;
      if (crossCategory) {
        const parsed = splitCategoryPath(selected);
        targetCategory = parsed.category;
        targetParent = parsed.categoryPath;
      } else {
        targetCategory = sourceCategory; // 单分类树，分类固定
        targetParent = selected;         // selected 已不含前缀
      }
      try {
        await api.moveFolder(kind, sourceCategory, folderPath, targetCategory, targetParent);
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
