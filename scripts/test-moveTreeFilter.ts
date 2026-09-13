import { matchTree } from "../src/lib/moveTreeFilter";
import type { TreeNode } from "../src/lib/videoTree";

function node(name: string, path: string, children: TreeNode[] = []): TreeNode {
  return { name, path, children, items: [] };
}

const root: TreeNode = node("电影", "", [
  node("科幻", "科幻", [ node("星战", "科幻/星战"), node("沙丘", "科幻/沙丘") ]),
  node("奇幻", "奇幻", [ node("哈利波特", "奇幻/哈利波特") ]),
]);

let failed = 0;
function assert(cond: boolean, msg: string) {
  if (!cond) { console.error("FAIL:", msg); failed++; } else { console.log("ok:", msg); }
}

{
  const r = matchTree(root, "");
  assert(r.visible.has("") && r.visible.has("科幻") && r.visible.has("科幻/星战") && r.visible.has("奇幻/哈利波特"),
    "空query全部可见");
}
{
  const r = matchTree(root, "星战");
  assert(r.visible.has("科幻/星战"), "命中项可见");
  assert(r.visible.has("科幻") && r.visible.has(""), "命中项祖先可见");
  assert(r.forceExpand.has("科幻") && r.forceExpand.has(""), "命中项祖先强制展开");
  assert(!r.visible.has("科幻/沙丘"), "同级未命中项不可见");
  assert(!r.visible.has("奇幻") && !r.visible.has("奇幻/哈利波特"), "无关分支不可见");
}
{
  const r = matchTree(root, "科幻");
  assert(r.visible.has("科幻") && r.visible.has("科幻/星战") && r.visible.has("科幻/沙丘"),
    "命中中间节点其整枝可见");
  assert(!r.visible.has("奇幻"), "无关分支不可见");
}
{
  const en: TreeNode = node("Movie", "", [ node("SciFi", "SciFi", [ node("Dune", "SciFi/Dune") ]) ]);
  const r = matchTree(en, "dune");
  assert(r.visible.has("SciFi/Dune"), "大小写不敏感命中");
}

if (failed) { console.error(`\n${failed} 个断言失败`); process.exit(1); }
console.log("\n全部通过");
