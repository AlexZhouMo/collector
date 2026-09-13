# 侧边栏折叠态展开钮微调设计

- 日期：2026-09-07
- 状态：设计已确认

## Context

侧边栏折叠后，展开钮（`.expand-btn`）当前是 `position:fixed; left:10px; top:14px`，浮在内容区左上角上方，会遮挡右侧内容面板；且固定在顶部而非垂直居中，视觉上偏。用户要求：折叠后展开钮不遮挡内容并留间距、垂直居中于窗口、比现在更窄。

## 改动（纯 CSS + main.ts 一行）

全部在 `src/styles/theme.css`，外加 `src/main.ts` 一处图标尺寸：

1. **展开钮垂直居中 + 缩窄** — `.expand-btn` 由 `top:14px; padding:6px` 改为：
   ```css
   .expand-btn{position:fixed;left:10px;top:50%;transform:translateY(-50%);z-index:50;padding:4px;border-radius:10px;display:none;color:var(--text)}
   ```
   即 `top:50% + translateY(-50%)` 窗口垂直居中，`padding:6px→4px` 缩窄。
   `src/main.ts` 里展开钮图标由 `icon("chevronRight", 18)` 改为 `icon("chevronRight", 16)`，进一步收窄。

2. **折叠时内容不遮挡 + 留间距** — 折叠态给内容区加左内边距为展开钮让位：
   ```css
   #app.sidebar-collapsed .content{padding-left:48px}
   ```
   展开钮宽约 24px（16px 图标 + 4px×2 padding + 边框），48px 左 padding 让内容避开它并留出间距。`.content` 本身 padding:20px（内联样式），折叠态用此规则覆盖左 padding；非折叠态不受影响。

## 明确不做（YAGNI）

- 不改折叠动画、不改展开钮的 hover/玻璃拟态样式。
- 不动侧边栏本身的折叠逻辑（toggleSidebar 不变）。

## 验证

`npm run build` 编译通过；应用热重载后：折叠侧边栏 → 展开钮位于屏幕左侧垂直居中、比之前窄；右侧内容不再被展开钮遮挡，内容左侧与展开钮之间有间距。点展开钮正常恢复。
