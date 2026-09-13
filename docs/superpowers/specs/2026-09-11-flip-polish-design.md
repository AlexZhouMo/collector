# 纯 CSS 翻书效果深度打磨 设计文档

**日期**：2026-09-11
**状态**：已确认

## 背景与动机

翻书已能播放（单页绕书脊 rotateY 0.6s + 一层线性阴影），但观感不够酷炫。用**纯 CSS**（无新依赖）
提升：阴影光影层次、缓动手感、相邻页联动、弯曲错觉。真·纸张卷曲需 WebGL/StPageFlip 库，本次不做——
"弯曲"用高光+阴影营造错觉（已与用户确认接受）。

## 四个增强

### 1. 阴影/光影层次
- 翻页中的页加多层动态叠层：正面翻走时投影随进度渐深（书脊侧深、外缘浅）；背面露出时带高光扫过带。
- 书脊中缝常驻柔和暗影（gutter shadow），双页对开中间一道暗影，强化"装订成书"立体感。
- 翻走的页在底层页上投下移动阴影（下方页面被翻起页遮挡处变暗）。

### 2. 相邻页联动（已确认）
- 翻页前，**底层先垫目标对开**（下一/上一对开的静态图），flipper（当前页）在其上翻转揭开——视觉上
  "掀开当前页露出下一页"，比现在"翻完才 render 换页"连贯。
- transitionend 后移除 flipper（底层已是目标页），减少整页重渲染的闪烁。

### 3. 时长/缓动手感
- 缓动曲线调为更有重量感（起步稍慢抓页角、中段加速、末尾轻微减速贴合）；时长 0.6s → 约 0.7s 更从容。

### 4. 弯曲错觉（已确认，纯 CSS）
- flipper 翻转中用沿翻转方向移动的渐变高光带（glare）模拟纸面反光；配合书脊侧更深阴影，感知为弯曲。
- 页面仍平板旋转，高光+阴影制造弯曲错觉，非真弯曲。

## 组件改动

### theme.css（主要）
- 强化 `.flipper .shade`（多层渐变 + 随翻转变化，用 keyframes 而非仅 opacity）。
- 新增高光层 `.flipper .glare`（翻转时移动的亮带）。
- 书脊中缝阴影：`.book` 上叠一道中线暗影（伪元素或独立层，双页时显示，single 时隐藏）。
- 底层页阴影：翻走页在底层投影。
- 缓动曲线与时长调整（约 0.7s，重量感 cubic-bezier）。
- 用 `@keyframes` 驱动 shade/glare 随翻转进度变化（配合 flipper 的 transform transition 同步时长）。

### ComicReaderView.ts go()
- **相邻页联动**：翻页时先把 book 的底层渲染为**目标对开**的静态页，再叠 flipper（内容=当前页正面 +
  目标页背面）在其上翻转；transitionend 后仅移除 flipper（底层已是目标页），不再整页 render（除非需
  更新 pager/nav——那些轻量更新保留）。
- flipper 结构加 `.glare` 层。
- 现有初始 transform:rotateY(0) + 双 rAF 触发（上次修复）保留。

## 交互整合

- 缩放/拖动/工具栏/铺满/双页 fit 全部保留不动。
- flipping 标志防重复触发；首末页 updateNav 边界；均保留。

## 错误处理

- 动画中忽略新翻页（现有）。
- 目标页图未缓存：load() 缓存机制保证取到（现有）。
- transitionend 多次触发（transform + 可能的其它 transition）：用 `{ once:true }` + 判断 propertyName
  为 transform 才收尾，避免 shade/glare 的 transition 提前触发收尾（若它们也有 transition）。

## 测试

- 纯视觉/动画：靠**真机验证**（预览 Chromium 与真机 WKWebView 的 3D transform/transition 表现不同，
  预览量数据不能证实观感）。tsc 保证无语法错。
- 结构性（flipper 层级、底层联动的 DOM 顺序、glare/shade 层存在）可预览 inspect 辅助。

## 影响与风险

- 仅前端：theme.css（主要）+ ComicReaderView.ts go()。无后端。
- **诚实标注**：纯 CSS 的"弯曲"是高光/阴影错觉，非真纸张卷曲。若真机看后仍要真弯曲，需 StPageFlip 库
  （本次不做）。
- **YAGNI**：不做翻页音效、不做拖拽跟手翻页（拖一半回弹）、不做真实卷曲物理。
