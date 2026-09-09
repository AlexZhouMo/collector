import type { TreeNode } from "./videoTree";

export interface MatchResult {
  visible: Set<string>;
  forceExpand: Set<string>;
}

/**
 * 保留树结构的搜索过滤：
 * - query 为空：所有节点可见，forceExpand 为空。
 * - query 非空（大小写不敏感，按 path 全串 includes）：
 *   节点可见 ⇔ 自身 path 含 query（其整棵子树也一并可见）或任一后代可见。
 *   命中项的祖先加入 forceExpand。
 */
export function matchTree(root: TreeNode, query: string): MatchResult {
  const visible = new Set<string>();
  const forceExpand = new Set<string>();
  const q = query.trim().toLowerCase();

  if (!q) {
    const all = (n: TreeNode) => { visible.add(n.path); n.children.forEach(all); };
    all(root);
    return { visible, forceExpand };
  }

  const walk = (node: TreeNode, ancestorHit: boolean): boolean => {
    const selfHit = node.path.toLowerCase().includes(q);
    const hit = ancestorHit || selfHit;
    let descVisible = false;
    for (const c of node.children) {
      if (walk(c, hit)) descVisible = true;
    }
    const nodeVisible = hit || descVisible;
    if (nodeVisible) visible.add(node.path);
    if (descVisible) forceExpand.add(node.path);
    return nodeVisible;
  };
  walk(root, false);
  return { visible, forceExpand };
}
