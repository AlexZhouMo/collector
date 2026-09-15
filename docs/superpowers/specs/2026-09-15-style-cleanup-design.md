# 样式文件清理（死代码 + 冗余 + 命名统一）· 设计

日期：2026-09-15
分支：master

## 背景与目标

对样式资源做审计（`theme.css`、`animations.css`、图片/字体/图标）。结论：
- **图片/字体/图标资源无孤儿可删**——app 图标全被 Tauri bundler（MSIX/`tauri icon` 约定）或 tauri.conf 引用需要；libass worker/wasm、CJK 字体、种子封面均为功能资源。这块不动。
- **CSS 有确定的死代码、字节级重复、分散定义**，以及一处命名不统一（`.add-video-btn` 被三类视图复用但名字带 video）。

本次做审计的 A+B+C：删死 CSS、去冗余、统一命名。行为与视觉零变化。

## 约束

- **视觉零变化**：删的都是零引用死规则；合并/去重都是等价的（属性不冲突）；改名的类在 CSS 中无专属样式定义，不影响渲染。
- **验证**：`npx tsc --noEmit` 通过；应用启动各页面视觉与清理前一致（尤其影视/漫画/游戏列表、播放器翻页、主页、侧边栏）。

## 涉及文件

- `src/styles/theme.css`、`src/styles/animations.css`
- `src/views/VideoView.ts`、`src/views/ComicView.ts`、`src/views/GameView.ts`（仅 C 项改名）

---

## A · 删除死 CSS（零引用，已 grep 确认）

1. **遗留单页/条带阅读器块** `theme.css:44-47`：`.stage.page`、`.page-img`、`.stage.strip`、`.strip-img`。`stage`/`page`/`strip` 类 token 在 TS 中从不出现（ComicReaderView 里的 `stage` 是局部 JS 变量，非 CSS 类）。整块删除。
2. **游戏卡片样式** `theme.css:80-87`：`.game-grid`、`.game-card`、`.game-cover`、`.game-cover img`、`.game-title`、`.game-desc`、`.game-na`、`.game-card.disabled`。GameView 实际复用 `video-view game-tree comic-tree` 类，这些 `.game-*` 从不生成。整组删除。
3. **死的 leaf-turn 翻页机制** `theme.css:288-289`：`.book .leaf.turn-in` 规则 + `@keyframes leafTurn`。已被 `.flipper` 机制取代，`turn-in` 从不通过 classList 添加。删除这两项。
   - 注意：`.leaf`、`.leaf.left`、`.leaf.right`、`.book.single .leaf` 仍在用（ComicReaderView `class="leaf left/right"`），**保留**，只删 `.turn-in` 变体与 `leafTurn` 关键帧。
4. **未用动画工具** `animations.css:3-4`：`.card-hover` 与 `.card-hover:hover`。TS 从不生成 `card-hover`。删除。

**保留确认**：`fadeInUp` 关键帧虽也被将删的 `.page-img` 引用，但通过 `.view-enter`（animations.css:2）存活，删 `.page-img` 后 `fadeInUp` 仍被 `.view-enter` 使用，**必须保留**。`.libassjs-canvas-parent`（theme.css:58）由 libass 运行时注入，非死代码，保留。

**验收**：四组死规则删除后 `grep` 确认无残留；应用各页面视觉不变；tsc 通过。

---

## B · 去除冗余（等价合并，属性不冲突）

1. **删字节级重复的 `.fv-video .poster-img`** `theme.css:146-147`：与 `.poster-img` / `.poster-img img`（`theme.css:36-37`）完全相同。因 `.poster-img` 已能在 `.fv-video` 内匹配，该后代限定副本冗余，删除 146-147 两行。
   - 保留确认：`.fv-video.disabled .poster-img` 等（156/157/159）是不同选择器，不受影响；`.fv-video` 内的 poster 基础样式由 36-37 继续匹配。
2. **合并分散定义的选择器**（各定义两处、属性不冲突，合并为一处以减少分散）：
   - `.sidebar`：`theme.css:28`（尺寸/padding/flex）+ `theme.css:123`（transition/overflow）。
   - `.brand`：`theme.css:29`（color/spacing）+ `theme.css:124`（display/justify）。
   - `.nav-item`：`theme.css:31`（padding/color/transition）+ `theme.css:129`（display/gap）。
   合并方式：把第二处的属性并入第一处的规则块，删除第二处。因两处属性互不重叠，合并后计算样式完全等价。
   - 注意：合并需确认两处之间没有依赖 CSS 源顺序的其它规则会因此改变优先级——这三个选择器都是单类选择器、特指度相同，属性不重叠，合并不改变任何层叠结果。

**验收**：合并后 `.sidebar`/`.brand`/`.nav-item` 各只定义一次；侧边栏布局、品牌区、导航项视觉与交互（折叠动画、hover、active）不变；tsc 通过。

---

## C · 命名统一：`.add-video-btn` → `.add-btn`

`.add-video-btn` 被视频/漫画/游戏三视图复用（新增按钮），名字带 `video` 但语义已泛化为「新增按钮」。且该类在 CSS 中**无任何样式定义**（样式来自默认 button），改名不影响渲染。

**改法**：把三个文件中的 `add-video-btn` 全部替换为 `add-btn`：
- `src/views/VideoView.ts`：markup `class="add-video-btn"`（行 65）+ `querySelector(".add-video-btn")`（行 78）。
- `src/views/ComicView.ts`：行 50 + 行 60。
- `src/views/GameView.ts`：行 59 + 行 69。
共 6 处。CSS 无需改（无 `.add-video-btn` 规则）。

**验收**：`grep -rn "add-video-btn" src/` 无残留；三视图新增按钮功能与外观不变；tsc 通过。

---

## 测试策略

- 前端无单测：靠 `tsc --noEmit` + 应用手动验证。
- 手动验证清单：主页、侧边栏（折叠/展开/hover/active）、影视/漫画/游戏列表（封面卡片、文件夹）、播放器翻页动画、三视图新增按钮显隐与外观——均与清理前一致。

## 非目标（YAGNI）

- 不删任何图片/字体/图标（均有引用或 bundler 需要）。
- 不做 CSS 文件结构性重排/分模块（审计未要求，且风险大于收益）。
- 不改动仍在使用的状态类、libass 注入类、种子资源。
- 不改 `.add-video-btn` 之外的类名（其余命名审计未发现不规范项）。
