# 阅读器铺满 + 底部按钮栏 + 真实翻书效果 设计文档

**日期**：2026-09-11
**状态**：已确认

## 背景与动机

3D 翻书阅读器需三项增强：中间漫画区去 padding 铺满；底部加按钮栏（翻页/页码/缩放/返回）；翻页从
简单进入动画（`turn-in` rotateY 进入）升级为真实翻书——翻转的页有正反两面、绕书脊卷动、带阴影。

## 布局（铺满）

`.comic-reader` 纵向三段：
- 顶栏 `.reader-bar`（标题/返回，保留）。
- 中间 `.book-stage`：`flex:1`、去 padding、铺满剩余空间；漫画高度自适应（不再 `calc(100vh-160px)` 固定，
  改为 flex 撑满，img `height:100%` 随之定高）。
- 底部 `.reader-toolbar`：新增按钮栏。

## 底部按钮栏 `.reader-toolbar`

玻璃拟态横条，居中排列（无"首页"）：
`‹ 上一页` · 页码 `{当前对开序}-... / {总对开数}` · `下一页 ›` · `− 缩小` · `＋ 放大` · `⊙ 适屏` · `返回`。
按钮绑定 ComicReaderView 已有的 go(±1) / zoomAt(缩放,以画面中心为锚) / resetZoom(适屏) / onExit(返回)。
首/末对开时上一页/下一页禁用（或 go 越界 return）。

## 真实翻书效果（CSS 3D 单页翻转）

翻页时构建一个临时 `.flipper` 元素叠在当前对开的右半页位置：
- `.flipper` 含 `.face.front`（当前右页图）+ `.face.back`（目标对开的对应页图，`rotateY(180deg)`）+ `.shade`（阴影层）。
- `transform-origin:left center`；`backface-visibility:hidden` 使正反面随翻转切换。
- 下一页：`.flipper` 动画 `rotateY(0 → -180deg)`，阴影渐显；上一页：反向（从 -180deg → 0，或对称构建）。
- `transitionend` 后移除 flipper、`render` 目标对开。翻转中用 `flipping` 标志防重复触发。
- 纯 CSS transform + 临时元素，无需第三方库。

实现要点：`go(d)` 改为——校验边界；resetZoom；构建 flipper（front=当前右页 url，back=目标页 url）；
加 flipping class 触发动画；transitionend/超时后 idx=目标、render()、移除 flipper、清 flipping。

## 交互整合

- 上次的缩放/拖动保留：`scale==1` 翻页（点击半屏/按钮/方向键），`scale>1` 拖动平移（翻页按钮翻页前 resetZoom）。
- 底部按钮与键盘/点击/滚轮各自绑定，无冲突；翻转动画期间忽略新翻页请求。

## 组件改动

### ComicReaderView.ts
- innerHTML 加底部 `.reader-toolbar`（7 个控件）。
- 绑定按钮：上一页/下一页→go(∓1)、缩小/放大→zoomAt(scale∓step, 画面中心)、适屏→resetZoom、返回→onExit。
- 翻页逻辑升级为 flipper 3D 翻转（go 内构建临时 flipper + transitionend 后 render）。
- 页码显示移到工具栏 pager（`idx+1 / spreads.length` 或对开范围）；首/末禁用翻页钮。

### theme.css
- `.comic-reader` flex 纵向；`.book-stage` flex:1 去 padding 铺满。
- `.reader-toolbar` 玻璃横条（居中、gap、按钮样式）。
- `.flipper`/`.face`/`.front`/`.back`/`.shade` 3D 翻转样式 + transition；保留/调整现有 book/leaf。

## 错误处理

- 首/末对开翻页 return 且按钮禁用态。
- flipping 期间忽略重复翻页。
- 目标页图加载失败：flipper 背面空白但不卡（transitionend 仍触发 render）。

## 测试

- 布局/翻书/按钮为 UI：预览 inspect（book-stage flex:1、reader-toolbar 存在、flipper transform）+ 截图 + 真机。
- 既有 buildSpreads 纯函数不变。翻页方向/边界靠 tsc + 真机。

## 影响与风险

- 仅前端：ComicReaderView.ts + theme.css。无后端改动。
- **YAGNI**：不做首页/跳页输入、不做翻页音效、不做连续翻页惯性、不引入翻页库、不做双面卷曲(仅平面 rotateY)。
