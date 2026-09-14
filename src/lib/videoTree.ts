import type { MediaItem } from "./ipc";

export interface TreeNode {
  name: string;          // 该层目录名（根为分类名）
  path: string;          // 分类内相对路径（不含分类名），根为空串 ""，如 "科幻"、"科幻/星战"
  children: TreeNode[];  // 子目录
  items: MediaItem[];    // 本目录直接包含的视频
}

/**
 * 把某分类下的扁平 items 按 category_path 构建为一棵树。
 * category 为分类名（如 "电影"），作为树根 name（根 path 为空串 ""）。
 * 每个 item 的 category_path 为「分类内相对路径」（不含分类名），
 * 形如 "科幻/星球大战"，各段即分类内目录层级；末段目录下挂该视频。
 * 若 category_path 为空串（视频直放分类根），挂到根的 items。
 */
export function buildVideoTree(category: string, items: MediaItem[]): TreeNode {
  const root: TreeNode = { name: category, path: "", children: [], items: [] };
  for (const it of items) {
    const inner = it.category_path ? it.category_path.split("/").filter(Boolean) : [];
    if (inner.length === 0) {
      root.items.push(it);
      continue;
    }
    let node = root;
    let prefix = "";
    for (const seg of inner) {
      prefix = prefix ? `${prefix}/${seg}` : seg;
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

/** 按完整 path 在树中查找节点，找不到返回 null。 */
export function findNode(root: TreeNode, path: string): TreeNode | null {
  if (root.path === path) return root;
  for (const c of root.children) {
    const found = findNode(c, path);
    if (found) return found;
  }
  return null;
}
