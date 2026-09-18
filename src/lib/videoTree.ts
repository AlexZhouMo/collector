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

/** 影视合一树的 path（带分类前缀）→ {category, categoryPath}。
 * "电影/科幻/诺兰" → {category:"电影", categoryPath:"科幻/诺兰"}
 * "电影" → {category:"电影", categoryPath:""} */
export function splitCategoryPath(prefixedPath: string): { category: string; categoryPath: string } {
  const i = prefixedPath.indexOf("/");
  if (i < 0) return { category: prefixedPath, categoryPath: "" };
  return { category: prefixedPath.slice(0, i), categoryPath: prefixedPath.slice(i + 1) };
}

/** 影视三分类合一树：虚拟根下挂电影/动漫/剧集三个分类子树。
 * 各分类节点 path=分类名（如"电影"）；其内部节点 path 加分类前缀（"电影/科幻"）。
 * 虚拟根 path="__root__"（仅容器，不作为移动目标）。
 * cats: [[category, items]]，如 [["电影", movieItems],["动漫",animeItems],["剧集",tvItems]] */
export function buildMergedVideoTree(cats: [string, MediaItem[]][]): TreeNode {
  const root: TreeNode = { name: "影视", path: "__root__", children: [], items: [] };
  for (const [cat, items] of cats) {
    const sub = buildVideoTree(cat, items); // 现有：根 path="", 内部 path 不含分类
    // 重写 path 加分类前缀：根→cat；内部原 path "科幻/诺兰" → "电影/科幻/诺兰"
    const reprefix = (n: TreeNode, isRoot: boolean) => {
      n.path = isRoot ? cat : `${cat}/${n.path}`;
      n.children.forEach(c => reprefix(c, false));
    };
    reprefix(sub, true);
    root.children.push(sub);
  }
  return root;
}
