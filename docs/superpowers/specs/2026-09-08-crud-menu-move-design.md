# CRUD 菜单精简 + 移动功能 + 编辑窗调整 设计

- 日期：2026-09-08
- 状态：设计待复审

## Context
面板视频 CRUD 已上线。本次调整右键菜单与编辑窗：右键菜单去掉「设置展示图」，保留编辑/删除，新增「移动」（把条目移到当前分类下已有的某个文件夹节点）；编辑窗「展示图」文案改「封面」，并去掉「分类」和「分类路径」两个字段（分类内的位置改由「移动」管理，分类切换不在编辑窗做）。

## 已确认决策
1. 右键菜单三项：编辑、移动、删除（删除保留 confirm）。移除「设置展示图」。
2. 移动：点「移动」弹菜单/下拉，列出当前分类（activeCat，如 电影）下**已有的所有文件夹节点**（从 buildVideoTree 的 TreeNode 递归收集 path），选一个 → mediaUpdate 把该条目 category_path 改为选中节点路径 → refresh。不手输路径、不新建目录。
3. 编辑窗：`展示图` label → `封面`；删除「分类」select 和「分类路径」input 两个字段（及其收集逻辑）。
4. 编辑窗保存时 category/category_path 不再来自表单：编辑已有条目时沿用原 item 的 category/category_path（不变）；新增条目时 category 用当前 activeCat、category_path 默认 = activeCat（放分类根，之后可用「移动」调位置）。

## 架构
- **videoTree.ts**：加 `collectFolderPaths(node: TreeNode): string[]`（递归收集所有文件夹节点的 path，去重、排序），供移动菜单列选项。
- **VideoView.ts**：
  - onContext 菜单改为：编辑 / 移动 / 删除。
  - 移动项 onClick：用 `collectFolderPaths(buildVideoTree(activeCat, 该分类items))` 得候选路径，`showContextMenu` 二级菜单（或复用一个选择列表）列出，选中 → `mediaUpdate(it.id, it.category, 选中path, it.title, it.path, it.subtitle_path, it.cover_path, it.description)` → refresh。
  - 编辑/新增仍调 openEditDrawer，但 openEditDrawer 需知道 activeCat（新增时默认分类）——传入 activeCat 参数。
- **EditDrawer.ts**：
  - `openEditDrawer(item, onSaved, defaultCategory)`：去掉分类 select、分类路径 input；`封面` label；保存时 category = item?.category ?? defaultCategory，category_path = item?.category_path ?? defaultCategory（新增放分类根）。其余字段（标题/视频/字幕/封面/简介）不变。

## 关键文件
- src/lib/videoTree.ts（collectFolderPaths）
- src/views/VideoView.ts（菜单改三项、移动逻辑、传 activeCat 给抽屉）
- src/components/EditDrawer.ts（去分类字段、封面文案、category 来源改）

## 明确不做
- 移动不新建目录（只选已有节点）；分类间移动不在本次（移动限当前分类内）
- 编辑窗不再改分类/路径（分类内位置用移动，跨分类暂不支持）

## 验证
1. 右键卡片菜单只有 编辑/移动/删除，无「设置展示图」。
2. 移动 → 列出当前分类下已有文件夹 → 选一个 → 条目移到该层级（refresh 后在新位置）。
3. 编辑窗无「分类」「分类路径」，「封面」文案正确；编辑保存后分类/路径不变；新增条目落在当前分类根。
4. 编译通过。
