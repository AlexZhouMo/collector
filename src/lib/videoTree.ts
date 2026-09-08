import type { MediaItem } from "./ipc";

export interface TreeNode {
  name: string;          // 该层目录名（根为分类名）
  path: string;          // 从分类根到本节点的完整 category_path 前缀，如 "电影/科幻"
  children: TreeNode[];  // 子目录
  items: MediaItem[];    // 本目录直接包含的视频
}

/**
 * 把某分类下的扁平 items 按 category_path 构建为一棵树。
 * category 为分类名（如 "电影"），作为树根 name。
 * 每个 item 的 category_path 形如 "电影/科幻/星球大战"，
 * 首段=分类名，其后为分类内的目录层级；末段目录下挂该视频。
 * 若 category_path 只有分类名一段（视频直放分类根），挂到根的 items。
 */
export function buildVideoTree(category: string, items: MediaItem[]): TreeNode {
  const root: TreeNode = { name: category, path: category, children: [], items: [] };
  for (const it of items) {
    const segs = it.category_path.split("/");
    const inner = segs.slice(1); // 去掉首段分类名
    if (inner.length === 0) {
      root.items.push(it);
      continue;
    }
    let node = root;
    let prefix = category;
    for (const seg of inner) {
      prefix = `${prefix}/${seg}`;
      let child = node.children.find(c => c.name === seg);
      if (!child) {
        child = { name: seg, path: prefix, children: [], items: [] };
        node.children.push(child);
      }
      node = child;
    }
    node.items.push(it);
  }
  sortTree(root);
  return root;
}

function sortTree(node: TreeNode) {
  node.children.sort((a, b) => a.name.localeCompare(b.name, "zh"));
  node.items.sort((a, b) => a.title.localeCompare(b.title, "zh"));
  node.children.forEach(sortTree);
}

/**
 * 收集树中所有文件夹节点的完整 path（含分类根），用于「移动」菜单。
 * 深度优先遍历，去重并按 path 排序（分类根排最前）。
 */
export function collectFolderPaths(root: TreeNode): string[] {
  const out: string[] = [];
  const walk = (node: TreeNode) => {
    out.push(node.path);
    node.children.forEach(walk);
  };
  walk(root);
  const uniq = Array.from(new Set(out));
  uniq.sort((a, b) => a.localeCompare(b, "zh"));
  return uniq;
}

/** 按完整 path 在树中查找节点，找不到返回 null。 */
export function findNode(root: TreeNode, path: string): TreeNode | null {
  if (root.path === path) return root;
  for (const c of root.children) {
    const found = findNode(c, path);
    if (found) return found;
  }
  return null;
}
