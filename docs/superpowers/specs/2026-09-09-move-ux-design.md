# 「移动」功能 UX 优化：搜索 + 树弹窗设计

## 背景与问题

视频右键菜单的「移动」当前实现（`src/views/VideoView.ts` `onContext`）把当前分类下所有文件夹节点用 `collectFolderPaths(buildVideoTree(...))` 平铺成一个二级 `showContextMenu`。电影分类实测有 108 个不同 `category_path`，平铺成 108 项的右键菜单无法使用——滚动长、无法搜索、层级信息丢失。

## 目标

用一个「搜索 + 保留树结构」的弹窗替代平铺二级菜单，让用户能：
1. 搜索框输入关键词即时过滤文件夹（保留树结构，命中项的祖先自动展开）。
2. 在树中逐层展开/折叠浏览，点击文件夹名选中目标（蓝色高亮）。
3. 点「移动到此」确认；移动成功后弹 toast 轻提示，弹窗关闭并刷新列表。

移动仍限当前分类内（电影/动漫/剧集各自的目录树），后端逻辑复用现有 `media_update`，仅改前端的目标选择方式。

## 决策汇总

| 项 | 决策 |
|----|------|
| 交互模式 | 搜索框 + 可展开树的模态弹窗（取代平铺二级菜单） |
| 搜索命中呈现 | **保留树结构**（命中项祖先自动展开），不拍平成列表 |
| 确认方式 | 选中目标后点「移动到此」按钮确认（非双击直接移动） |
| 移动结果反馈 | 成功后弹 **toast** 轻提示（新增通用 toast 组件） |
| 当前所在文件夹 | 标「当前位置」、灰置、不可选 |
| 分类根 | 作为可选目标，显示为「<分类名>（根）」，置于树顶 |
| 移动范围 | 仅当前分类内（不跨电影/动漫/剧集） |
| 后端 | 复用 `api.mediaUpdate(id, category, targetPath, title, subtitle_path, cover_path, description)`，无新增命令 |

## 组件与文件

### 新增 `src/components/MoveDialog.ts`

导出 `openMoveDialog(item, tree, onMoved)`：

```ts
export function openMoveDialog(
  item: MediaItem,          // 被移动的视频（提供 id / category / category_path / title 等）
  tree: TreeNode,           // 当前分类的完整目录树（buildVideoTree 产出）
  onMoved: () => void       // 移动成功后回调（VideoView 传 refresh）
): void
```

内部状态：
- `query: string`——搜索框内容。
- `expanded: Set<string>`——已展开节点的 path 集，初始只含根 `""`。
- `selected: string | null`——当前选中的目标 path，初始 `null`（未选，确认按钮禁用）。

**结构**（复用 `.glass` 玻璃风 + 遮罩层 `.modal-overlay`）：
1. 标题：`移动「<displayTitle(item.title)>」到…`
2. 搜索框 `<input class="move-search">`，`oninput` 更新 `query` 后重渲染树。
3. 树容器 `.move-tree`（`max-height` + `overflow:auto`）：递归渲染，行结构参照 `TreeView` 的 `renderTreeNodes`（`▾/▸/　` 箭头 + 名称，`padding-left = depth*14+4`）。
   - 根节点名显示为 `${tree.name}（根）`。
   - 当前所在文件夹（`node.path === item.category_path`）：加 `.cur` 类，追加「当前位置」标签，不响应选中点击。
   - 选中节点加 `.sel` 高亮。
4. 提示行 `.move-hint`：「搜索缩小范围；点箭头展开/折叠；点名称选中目标」。
5. 底部 `.move-actions`：`取消` 按钮 + `移动到「<选中文件夹名>」`（未选中时 disabled 且文案为「请选择目标文件夹」）。

**搜索过滤（保留树结构）**：`query` 非空时，一个节点可见当且仅当「它或它的任一后代的 path 含 query（不区分大小写，按 path 全串匹配）」。命中节点的所有祖先在渲染时强制视为展开（无论 `expanded` 状态），使命中项可见。`query` 为空时恢复按 `expanded` 正常展开/折叠。用一个纯函数 `matchTree(node, query) -> { visible: boolean }` 递归计算可见性。

**交互**：
- 点箭头：切换 `expanded`，重渲染（`query` 为空时生效；`query` 非空时展开态由匹配决定，箭头仅作视觉）。
- 点节点名（非 `.cur`）：设 `selected = node.path`，重渲染更新高亮和按钮文案。
- 点「移动到此」：`selected` 为 `null` 时不可点；否则调
  `await api.mediaUpdate(item.id, item.category, selected, item.title, item.subtitle_path, item.cover_path, item.description)`；成功 → 关闭弹窗 → `showToast('已移动到「<目标名>」')` → `onMoved()`；失败 → `showToast('移动失败：'+e, 'error')`，弹窗不关。
- 点「取消」、点遮罩层、按 Esc：关闭弹窗，不移动。

「目标名」= 选中节点为根时用 `tree.name`，否则用节点 `name`（末段目录名）。

### 新增 `src/components/Toast.ts`

导出 `showToast(message, kind?)`：

```ts
export function showToast(message: string, kind?: "info" | "error"): void
```

在 `document.body` 上追加一个 `.toast`（`.toast-error` 变体），固定屏幕底部居中，2.5s 后淡出移除。多次调用时新 toast 叠加（后者在上）或替换前者——本设计取**替换**：调用时先移除已有 `.toast`，保证同一时刻只一个。

### 修改 `src/views/VideoView.ts`

`onContext` 里「移动」菜单项的 `onClick` 从「构建平铺 `menuItems` + 二级 `showContextMenu`」改为：

```ts
{
  label: "移动",
  onClick: () => {
    const tree = buildVideoTree(activeCat, items.filter(i => i.category === activeCat));
    openMoveDialog(it, tree, refresh);
  },
}
```

删除原 `collectFolderPaths` 平铺映射逻辑。顶部 import：加 `openMoveDialog`；`collectFolderPaths` 若不再被其它处使用则从 import 移除（`buildVideoTree` 仍用）。

### 样式 `src/styles.css`（或现有样式文件）

新增 `.modal-overlay`（全屏半透明遮罩，flex 居中）、`.move-dialog`（复用 glass，宽约 420px）、`.move-search`、`.move-tree` / `.move-node` / `.move-node.sel` / `.move-node.cur` / `.move-tag`、`.move-hint`、`.move-actions` / `.btn` / `.btn.primary` / `.btn.disabled`、`.toast` / `.toast-error`。配色沿用现有 CSS 变量（`--glass` / `--border` / `--accent` 等）。

## 数据流

1. 右键视频 → 菜单「移动」→ `openMoveDialog(item, tree, refresh)`。
2. 用户搜索/展开/点选目标文件夹 → `selected` 更新。
3. 点「移动到此」→ `api.mediaUpdate(...selected...)`（后端 `media_update` 改 `category_path`，`category` 不变）。
4. 成功 → 关闭弹窗 → `showToast` → `refresh()`（重新 `list_media` 渲染，视频出现在新文件夹）。

## 错误处理

- `mediaUpdate` 抛错（如目标已存在同名——`UNIQUE(category_path,title)` 冲突）：`showToast('移动失败：'+e, 'error')`，弹窗保持打开，用户可改选或取消。
- 空树 / 只有根：树仍渲染根节点，用户可把视频移到根。
- 选中即当前位置：该节点不可选（`.cur`），不会触发移动。

## 测试策略

- **纯函数单测**（`MoveDialog` 的过滤逻辑抽成可测函数 `matchTree(node, query)`）：验证保留树结构的可见性——命中叶子时其祖先可见、无关分支不可见、空 query 全可见。放 `src/lib/` 便于测试，或在 `MoveDialog.ts` 内导出。
- `tsc` 通过。
- 真机：电影分类（108 文件夹）右键移动 → 弹窗打开默认只展开第一层；搜关键词能过滤并展开命中；点选目标 + 移动到此 → toast 提示 + 视频出现在目标文件夹；当前位置灰置不可选；取消/Esc/点遮罩关闭；移到已有同名触发失败 toast 且弹窗不关。
- 动漫/剧集分类同样可用。

## 明确不做（YAGNI）

- 不支持跨分类移动（电影→动漫）。
- 不支持在弹窗内新建文件夹。
- 不支持多选批量移动（本次仅单条）。
- 不做双击直接移动、不做搜索结果拍平列表（已决策为点击确认 + 保留树结构）。
- 不做 toast 队列/堆叠（同一时刻只一个）。

## 风险

- 树过滤需保证命中项祖先展开的逻辑正确，否则搜到的文件夹看不见——用纯函数单测覆盖。
- `collectFolderPaths` 若无其它调用者，移除 import 需确认（避免 tsc 未使用告警）。
- toast 为纯前端新增，无后端依赖，风险低。
