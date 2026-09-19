# 新增条目时可编辑目录路径（支持落到任意/新建目录）

日期：2026-09-20

## Context（为什么做这件事）

需求：新增视频/漫画/游戏时，能把内容放到任意目录（含尚不存在的新目录）；新增按钮在任何目录页都显示。

背景约束（上轮已探明）：目录树完全从条目的 `category_path` 派生，数据库无独立文件夹表——**空文件夹无法持久存在**。因此"新建目录"不能是建空目录，而是**新增条目时把 category_path 设成新路径**，条目落进去后该目录自然出现在树里（`buildVideoTree` 派生）。这正是本需求的巧妙之处：通过"可编辑路径 + 新增条目"间接实现建目录。

## 现状对照

- **新增按钮显示条件**：`MediaLibraryView` 的 `updateAddBtn` 现在仅在 `mode==="folder" && folderPath !== ""`（进入子目录后）显示，一级根目录页隐藏。→ 需改。
- **路径可编辑**：`EditDrawer` 现在 `categoryPath` 是写死常量（`item?.category_path ?? createPath ?? defaultCategory`），表单里**无路径输入框**。→ 需新增。
- **新路径自动显示新文件夹**：**已实现**。EditDrawer 保存 → `onSaved()`（= MediaLibraryView.refresh）→ 重新 listMedia + 重建树 → 新 category_path 被 `buildVideoTree` 派生出新文件夹节点。无需改动。

## 已确认的决策

- 路径文本框填**分类内目录路径**（不含分类名、不含视频文件名，如 `犯罪题材/教父`），与现有 category_path 语义一致；默认填当前目录，可任意改。
- 校验：**基本非法字符校验**——禁止 `\ : * ? " < > |`、不以 `/` 开头或结尾、不含空段 `//`、不含 `..`；允许中文、多层 `a/b/c`。
- 新增按钮：folder 模式任何目录都显示；**tree 模式也显示**（默认填分类根）。
- 可编辑路径框**只在新增时提供**；编辑现有条目保持原行为（不在此需求内）。

## 改动

### 1. 新增按钮任何目录都显示（`src/views/MediaLibraryView.ts`）
- `updateAddBtn` 的显示条件从 `mode==="folder" && folderPath !== ""` 改为**始终显示**（folder 与 tree 模式都显示）。
- 传给 EditDrawer 的默认路径：folder 模式用 `folderPath`（当前浏览目录）；tree 模式用分类根（空串或 activeCat 根）。影视用 editCategory() 作分类。

### 2. EditDrawer 新增时加可编辑路径框（`src/components/EditDrawer.ts`）
- 新增（item===null）时渲染一个路径文本框 `<input data-f="path">`，默认值 = 传入的 createPath（当前目录）。
- 实时校验：输入变化时用 `isValidCategoryPath` 校验，非法则禁用保存按钮 + 显示提示（如"路径含非法字符"）。
- 保存时用文本框的值作 categoryPath 传给 mediaCreate（替代原写死常量）。
- 编辑（item 有值）时不渲染路径框，保持原逻辑。

### 3. 路径校验纯函数（`src/lib/` 新增或就近）
```typescript
/** 分类内目录路径合法性：禁非法字符、不以/开头结尾、无//空段、无 ..。允许中文、多层。空串合法（=分类根）。 */
export function isValidCategoryPath(path: string): boolean
```
可单测（纯函数）。

## 验证方式

- 一级根目录页：新增按钮可见（之前隐藏）。
- 新增视频，路径框默认填当前目录；改为新路径如 `测试题材/新片` → 保存 → 列表出现"测试题材"文件夹含该视频（第 3 点机制）。
- 非法路径（`a//b`、`/x`、`a:b`、`../x`）→ 保存按钮禁用 + 提示。
- 合法路径（`犯罪题材/教父`、中文多层）→ 可保存。
- 漫画/游戏同样：新增时可编辑路径、落到新目录显示。
- `npx tsc --noEmit`、`npm run build`、`cargo test`（若加后端校验测试）全过。

## 现有可复用
- `EditDrawer` 的 createPath 参数（已存在，现作写死 categoryPath，改为路径框默认值）。
- `MediaLibraryView` 的 refresh → 树重建机制（第 3 点已实现，不改）。
- `buildVideoTree` 从 category_path 派生目录（不改）。

## 范围外
- 不建空文件夹（架构限制，且本方案通过"新增条目落新路径"已满足需求）。
- 编辑现有条目不加可编辑路径框（移动条目用现有移动功能）。
