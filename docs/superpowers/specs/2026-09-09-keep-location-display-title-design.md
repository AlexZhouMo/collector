# 编辑保存后保留浏览位置 + 视频名称去年份展示

## 目标

两个前端调整：
1. 编辑视频保存后关闭编辑框（现状已关），但刷新时**保留当前所在文件夹/树节点**，不跳回分类根目录。
2. 面板中视频卡片下方的名称，去掉开头的年份前缀（仅前端展示，不改数据库/编辑框）。

## 决策汇总

| 项 | 决策 |
|----|------|
| 保存后跳根 | 提升浏览位置为 VideoView 状态，refresh 重建时传回，保留位置 |
| 切分类/换视图 | 位置重置为分类根（合理） |
| 去年份范围 | 仅去开头 `[YYYY].` / 裸 `YYYY.` 前缀，保留片名中间/结尾数字 |
| 去年份作用域 | 仅面板卡片展示名；DB title、编辑框、搜索分组均用原 title |

## 根因（需求1）

`refresh → render` 重建 `.video-body`，每次 new 出 FolderView/TreeView，其内部 `currentPath`（当前文件夹）/`selected`（树节点）重置回根 → 保存后跳回分类根。

## 改动方案

### 需求1：保留浏览位置

**VideoView.ts**：
- 加状态 `let folderPath = activeCat`（文件夹视图当前路径）、`let treeSelected = activeCat`（树视图选中节点）。
- render 时把对应位置作为初始值传入组件；组件位置变化时回调同步回这两个变量。
- 切换分类 tab：`folderPath = activeCat; treeSelected = activeCat`（重置到新分类根）后 render。
- 切换视图模式：不重置（各自保留自己的位置状态）。

**FolderView.ts**：
- 签名加参数：`FolderView(root, onOpen, onContext, initialPath?, onNav?)`。
- `currentPath` 用 `initialPath ?? root.path` 初始化；若 initialPath 在当前 root 中不存在（如刚移动/删除导致节点没了），回退到 root.path（用 findNode 判断）。
- 每次 currentPath 变化（进文件夹/点面包屑）调 `onNav?.(currentPath)`。

**TreeView.ts**：
- 签名加 `initialSelected?`、`onNav?`；`selected` 用 initialSelected 初始化（findNode 校验存在性，否则 root.path）；expanded 也可据此展开到该节点的祖先链（可选，先保证 selected 保留即可）；selected 变化调 onNav。

### 需求2：视频名称去年份展示

- 新增纯函数 `src/lib/displayTitle.ts`：
  ```ts
  /** 去掉开头的年份前缀（[YYYY]. 或裸 YYYY.），仅供展示。保留片名中间/结尾数字。 */
  export function displayTitle(title: string): string {
    return title.replace(/^\[(\d{4})\]\.?/, "").replace(/^(\d{4})\./, "").trim() || title;
  }
  ```
  （注：先试 `[YYYY]` 前缀，再试裸 `YYYY.`；空结果回退原 title 防意外清空。）
- 三处展示改用 `displayTitle(it.title)`：
  - `PosterGrid.ts:15` `.poster-title`
  - `FolderView.ts:38` `.fv-name`（视频名；文件夹名 c.name 不动）
  - `TreeView.ts:37` `.poster-title`
  - 占位图 `poster-ph`（无封面时显示 title 的三处）也一并用 displayTitle，保持一致。
- 编辑抽屉、mediaUpdate、分组、TMDB 搜索等**均不改**，仍用原始 title。

## 测试策略

- `displayTitle` 单测思路（前端纯函数）：`[2015].非常人贩` → `非常人贩`；`2006.寂静岭` → `寂静岭`；`玩命快递3` → `玩命快递3`（不变）；`铁血战士2010` → `铁血战士2010`（不变）；无前缀原样。
- 真机：
  1. 进入某文件夹深层 → 编辑一个视频 → 保存 → 停留在原文件夹，不跳根。
  2. 树视图选中某节点 → 编辑保存 → 仍选中该节点。
  3. 卡片名称不含开头年份；编辑框内 title 仍含年份（可编辑真实值）。
  4. 移动/删除后位置若目标节点已不存在，回退分类根不报错。

## 明确不做（YAGNI）

- 不改数据库 title。
- 不去片名中间/结尾数字。
- 不改编辑框、搜索、分组的 title 使用。
- 树视图祖先链自动展开为可选增强，非必须（保证 selected 保留即可）。

## 风险

- initialPath/initialSelected 指向的节点在刷新后可能已不存在（移动/删除）——用 findNode 校验，不存在回退 root.path，避免空白。
- displayTitle 正则仅匹配开头，保留片名数字，符合"只去开头年份"决策。
