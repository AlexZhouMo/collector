# 文案变更 + 禁用默认右键菜单

## 目标

两组小改动：
1. 文案：工具箱「海报」卡片标题改为「影视海报生成」；左侧菜单「视频」改为「影视」。
2. 交互：除视频条目的 CRUD 编辑菜单外，应用其他任何地方右键不再弹出 WebView 默认系统菜单。

## 决策汇总

| 项 | 决策 |
|----|------|
| 文案范围 | 只改这两处用户可见文本，其余「视频」文案（设置页卡片、编辑抽屉等）不变 |
| 内部标识符 | 路由 key、图标名、后端 kind="video"、settings key（video_movie 等）一律不动 |
| 右键拦截 | main.ts 全局 document contextmenu 监听 preventDefault，一律禁止 |
| 输入框例外 | 不做例外，全局一律禁止（符合「其他地方右键不弹任何系统菜单」）；Cmd+C/V 快捷键不受影响 |

## 改动方案

### A. 文案

1. `src/views/NormalizeView.ts:33`
   `<span class="setting-card-title">海报</span>` → `<span class="setting-card-title">影视海报生成</span>`

2. `src/components/Sidebar.ts:7`
   `["video", "视频", "video"]` → `["video", "影视", "video"]`
   （仅改中间显示名；路由 key `"video"` 和图标名 `"video"` 不动）

### B. 全局禁用默认右键菜单

`src/main.ts` 顶部（入口初始化处）加：
```ts
// 全局禁用 WebView 默认右键菜单（重新加载/检查/复制图片等）。
// 视频条目 CRUD 菜单在元素 oncontextmenu 里主动弹出，不依赖系统菜单，不受影响。
document.addEventListener("contextmenu", (e) => e.preventDefault());
```

## 为什么不影响 CRUD 菜单

视频条目（FolderView/PosterGrid/TreeView）在各自元素的 `oncontextmenu` 里已 `preventDefault()` 并主动调 `showContextMenu(...)` 弹出自绘菜单。全局监听只是兜底阻止**默认系统菜单**，与主动弹出的自绘菜单互不冲突。

## 测试策略

纯 UI 文案 + 事件拦截，无单元测试。真机验证：
1. 左侧菜单显示「影视」；工具箱卡片标题显示「影视海报生成」。
2. 视频海报上右键 → 仍弹出编辑/移动/删除菜单。
3. 空白区域、图片、文字上右键 → 无任何系统菜单弹出。
4. 编辑抽屉输入框右键无系统菜单，但 Cmd+C/V 仍可用。

## 明确不做（YAGNI）

- 不改内部标识符（路由/图标/kind/settings key）。
- 不改其余「视频」文案（设置页、编辑抽屉等）。
- 输入框右键不做保留复制粘贴的例外。
