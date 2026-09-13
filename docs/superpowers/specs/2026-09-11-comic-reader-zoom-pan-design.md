# 漫画阅读器不变形 + 缩放拖动 + 双页无缝整体 设计文档

**日期**：2026-09-11
**状态**：已确认

## 背景与动机

3D 翻书阅读器需优化显示与交互：图片保持宽高比不变形（定高、宽自适应）；竖单页配对的双页无缝
拼成一个整体；支持滚轮/双击缩放 + 放大后拖动平移；未放大时保持点击/方向键翻页。

## 现状

- `.book .leaf img{height:100%;width:auto}`：已是"定高、宽自适应"。若真机变形，需确保 img 不受
  其它宽度约束、加 `object-fit:contain` 兜底、`.leaf` 不设固定宽。
- 左右两 leaf 独立，中间可能有间隙；要 `gap:0`、img `display:block` 消除间隙，视觉连成整体。
- 无缩放/拖动交互。

## 交互模型（ComicReaderView 新增状态）

维护 `scale`（初 1）、`tx`/`ty`（平移，初 0），应用到 `.book`：
`transform: translate(txpx, typx) scale(scale)`。缩放/平移作用于整个 `.book`（= 整个双页），
满足"整体缩放"。

- **滚轮 wheel**：以光标位置为锚调整 scale（步进约 0.15，钳制 [1, 4]），调整 tx/ty 使光标下的点
  不动；`preventDefault` 阻止页面滚动。scale 回到 1 时 tx/ty 归 0。
- **双击 dblclick**：scale==1 → 放大到 2（以双击点为锚）；否则复位（scale=1, tx=ty=0）。
- **拖动**：仅 `scale>1` 时，mousedown 记起点 → mousemove 改 tx/ty → mouseup 结束。`scale==1` 不拖动。
- **翻页**：`scale==1` 时左/右半屏点击翻页（现有逻辑，需与双击、拖动区分——click 且 scale==1 且未
  发生拖动才翻页）；方向键任何时候可翻页，翻页时重置 scale=1/tx=ty=0。
- **边界**：scale 钳制 [1,4]；scale==1 时 tx/ty 强制 0。tx/ty 首版做简单钳制（不把双页中心拖出
  stage 太远），或不钳制（YAGNI，倾向简单钳制到 ±(尺寸*(scale-1)/2)）。

## 双页无缝整体

- `.book` flex 容器 `gap:0`，左右 leaf 无 margin/border 间隙；img `display:block;height:100%;
  width:auto;object-fit:contain`。竖单页配对两张并排即成一整张对开。
- transform 作用于 `.book` 整体 → 两页一起缩放平移，等同一张对开图。
- 缩放/平移用 CSS transform；拖动时去掉 transition（跟手），缩放/复位时可带 transition（平滑）。

## 组件改动

### ComicReaderView.ts
- 新增 scale/tx/ty 状态 + `applyTransform()`（设 book.style.transform）。
- `wheel`（锚点缩放）、`dblclick`（放大/复位）、`mousedown/mousemove/mouseup`（scale>1 拖动）、
  `click`（scale==1 且未拖动 → 翻页）、方向键（翻页 + 重置变换）。
- 每次 `render()`（翻页/切对开）后重置 scale=1/tx=ty=0 并 applyTransform。
- `_cleanup` 增加移除新增的 window/stage 监听（wheel/mousemove/mouseup/keydown）。

### theme.css
- `.book` 加 `gap:0`；缩放变换加 `transition:transform .15s`（拖动时用 class 关掉）。
- `.book .leaf img` 确保 `height:100%;width:auto;object-fit:contain;display:block`。
- `.book-stage` `overflow:hidden`（放大后不溢出容器）；缩放时 `cursor:grab`，拖动中 `grabbing`。

## 错误处理

- scale 钳制 [1,4]；scale==1 强制 tx=ty=0。
- 拖动仅 scale>1 生效，未放大时鼠标按下不进拖动模式，click 照常翻页（避免翻页/拖动冲突）。
- 双击与单击翻页：dblclick 独立处理；单击翻页在 click 里判 scale==1 且本次非拖动。

## 测试

- 变形/无缝为 CSS：预览 inspect（img 的 width/height/object-fit、leaf gap=0）+ 截图 + 真机。
- 交互逻辑（缩放钳制、翻页重置、拖动仅放大生效）：项目无前端测试框架，靠 tsc + 真机验证；
  可选把"锚点缩放后 tx/ty 计算"抽纯函数（若加测试框架则单测，否则内联）。

## 影响与风险

- 仅前端：ComicReaderView.ts（交互）+ theme.css（样式）。无后端/ipc 改动。
- **YAGNI**：不做触控板双指 pinch（首版滚轮）、不做缩放惯性/回弹、不做每页独立缩放、不做拖动
  越界动画。
