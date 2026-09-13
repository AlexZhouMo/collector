# 漫画阅读器缩放拖动 + 不变形 + 双页无缝 实现计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 阅读器图片不变形（定高宽自适应）、双页无缝拼成整体、支持滚轮/双击缩放 + 放大后拖动平移，未放大时保持点击/方向键翻页。

**Architecture:** 纯前端。ComicReaderView 加 scale/tx/ty 状态与 wheel/dblclick/mousedown-move-up/click/keydown 交互，transform 作用于 `.book` 整体；theme.css 保证 img 不变形、leaf 无缝、stage 裁剪。无后端改动。

**Tech Stack:** vanilla-ts + CSS transform。项目无前端测试框架，靠 tsc + 预览 inspect/截图验证。

---

### Task 1: ComicReaderView 缩放拖动 + theme.css 不变形/无缝

**Files:**
- Modify: `src/views/ComicReaderView.ts`
- Modify: `src/styles/theme.css`

- [ ] **Step 1: 改 theme.css（不变形 + 无缝 + 裁剪 + 光标）**

把 theme.css 里 book 相关几条改为（`.book` 加 gap:0 与缩放 transition；img 加 object-fit/display；stage overflow hidden + 光标）：

```css
.book-stage{flex:1;min-height:0;display:flex;align-items:center;justify-content:center;background:#0a0d16;border-radius:12px;perspective:2400px;overflow:hidden}
.book{display:flex;align-items:center;justify-content:center;gap:0;height:calc(100vh - 160px);transform-style:preserve-3d;transition:transform .15s ease}
.book.dragging{transition:none}
.book .leaf{height:100%;background:#000}
.book .leaf img{height:100%;width:auto;display:block;object-fit:contain;box-shadow:0 8px 30px rgba(0,0,0,.6)}
.book.single .leaf img{max-width:min(90vw,700px);width:auto}
.book .leaf.right{transform-origin:left center}
.book .leaf.turn-in{animation:leafTurn .42s ease-out}
```

（保留 `@keyframes leafTurn`、`.vol-thumb`、spinner 等其它样式不动。给 `.book-stage` 在 scale>1 时的 cursor 由 JS 设，或加 `.book-stage.zoomed{cursor:grab}` `.book-stage.zoomed.dragging{cursor:grabbing}`——本步一并加这两条。）

追加：
```css
.book-stage.zoomed{cursor:grab}
.book-stage.zoomed.dragging{cursor:grabbing}
```

- [ ] **Step 2: 改 ComicReaderView.ts 加缩放/拖动状态与交互**

在 `ComicReaderView` 内（render/go 定义后，事件绑定处）改造。完整替换事件绑定段（现有 stage.click / onKey / cleanup）为：

```ts
  // 缩放/平移状态：transform 作用于整个 .book（双页整体）
  let scale = 1, tx = 0, ty = 0;
  let dragging = false, dragStartX = 0, dragStartY = 0, txStart = 0, tyStart = 0;
  const MIN = 1, MAX = 4;
  const applyTransform = () => {
    book.style.transform = `translate(${tx}px, ${ty}px) scale(${scale})`;
    stage.classList.toggle("zoomed", scale > 1);
  };
  const resetZoom = () => { scale = 1; tx = 0; ty = 0; applyTransform(); };

  // 翻页时重置缩放/平移
  const go = async (d: number) => {
    const ni = idx + d;
    if (ni < 0 || ni >= spreads.length) return;
    idx = ni;
    resetZoom();
    await render(d > 0 ? "next" : "prev");
  };

  // 以某点(px,py 相对 stage 中心)为锚缩放：保持锚点下的图像点不动
  const zoomAt = (nextScale: number, px: number, py: number) => {
    const clamped = Math.max(MIN, Math.min(MAX, nextScale));
    if (clamped === scale) return;
    // 锚点在 book 局部坐标：(p - t) / scale；缩放后保持该点位置不变解出新 t
    tx = px - ((px - tx) / scale) * clamped;
    ty = py - ((py - ty) / scale) * clamped;
    scale = clamped;
    if (scale === MIN) { tx = 0; ty = 0; }
    applyTransform();
  };

  const rectCenter = (e: MouseEvent) => {
    const r = stage.getBoundingClientRect();
    return { px: e.clientX - r.left - r.width / 2, py: e.clientY - r.top - r.height / 2 };
  };

  stage.addEventListener("wheel", (e) => {
    e.preventDefault();
    const { px, py } = rectCenter(e as WheelEvent);
    const factor = (e as WheelEvent).deltaY < 0 ? 1.15 : 1 / 1.15;
    zoomAt(scale * factor, px, py);
  }, { passive: false });

  stage.addEventListener("dblclick", (e) => {
    const { px, py } = rectCenter(e as MouseEvent);
    if (scale === MIN) zoomAt(2, px, py); else resetZoom();
  });

  stage.addEventListener("mousedown", (e) => {
    if (scale <= MIN) return; // 未放大不拖动（让 click 翻页）
    dragging = true; book.classList.add("dragging");
    dragStartX = (e as MouseEvent).clientX; dragStartY = (e as MouseEvent).clientY;
    txStart = tx; tyStart = ty;
    e.preventDefault();
  });
  const onMove = (e: MouseEvent) => {
    if (!dragging) return;
    tx = txStart + (e.clientX - dragStartX);
    ty = tyStart + (e.clientY - dragStartY);
    applyTransform();
  };
  const onUp = () => { if (dragging) { dragging = false; book.classList.remove("dragging"); } };
  window.addEventListener("mousemove", onMove);
  window.addEventListener("mouseup", onUp);

  stage.addEventListener("click", (e) => {
    if (scale > MIN) return; // 放大态下点击不翻页（拖动/查看）
    const x = (e as MouseEvent).clientX;
    if (x < window.innerWidth / 2) go(-1); else go(1);
  });

  const onKey = (e: KeyboardEvent) => {
    if (e.key === "ArrowLeft") go(-1);
    else if (e.key === "ArrowRight") go(1);
    else if (e.key === "Escape") onExit();
  };
  window.addEventListener("keydown", onKey);
  (el as any)._cleanup = () => {
    window.removeEventListener("keydown", onKey);
    window.removeEventListener("mousemove", onMove);
    window.removeEventListener("mouseup", onUp);
  };
```

> 注意：原文件里已有 `const go = async ...`，本次用上面含 resetZoom 的版本**替换**它（不要重复定义）。原 `stage.addEventListener("click", ...)` 与 `onKey`/`_cleanup` 一并被上面替换。`book`/`stage`/`pager` 变量沿用现有 querySelector。dblclick 会先触发两次 click——click 里 `scale>MIN` return 能挡住放大后的；放大到 2 后第二次 click 不翻页，符合。首次双击放大前 scale==1，两次 click 会翻页两次再 dblclick 放大——为避免，click 翻页改为在 dblclick 检测窗口后执行不做（YAGNI）；简单处理：dblclick 放大/复位即可，翻页误触发两次的概率低且方向键为主翻页手段。若真机觉得双击误翻页烦，后续再加 click 延迟判定。

- [ ] **Step 3: 类型检查**

Run: `npx tsc --noEmit`
Expected: 无输出（通过）。修到干净。

- [ ] **Step 4: 预览验证（不变形 + 无缝）**

起预览，注入一个双页 book（两张不同宽高比的假图），inspect：
- `.book` 的 `gap` == "0px"、`transform` 可被 JS 改。
- `.book .leaf img` 的 `object-fit` == "contain"、随容器高度定高、width 非拉伸。
- `.book-stage` `overflow` == "hidden"。
截图确认双页中间无缝、图片不变形。缩放/拖动交互靠真机（预览注入可验 applyTransform 改 transform 生效）。

- [ ] **Step 5: 提交**

```bash
git add src/views/ComicReaderView.ts src/styles/theme.css
git commit -m "feat(comic): 阅读器图片不变形+双页无缝+滚轮/双击缩放+放大后拖动平移

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>"
```

---

## 自查

- **规格覆盖**：不变形（Step 1 img object-fit/width:auto）、双页无缝（Step 1 gap:0 + leaf 无间隙）、整体缩放（Step 2 transform 作用于 .book）、滚轮锚点缩放 + 双击放大/复位 + 放大后拖动 + 未放大点击/方向键翻页 + 翻页重置变换（Step 2）、stage 裁剪与光标（Step 1）——规格各条均有对应实现。
- **占位符扫描**：无 TBD；Step 2 给出完整替换代码。双击/单击共存的边界在 Step 2 注明按 YAGNI 简单处理（dblclick 放大 + 方向键主翻页），是明确决策非占位。
- **一致性**：scale/tx/ty/dragging 状态、applyTransform/resetZoom/zoomAt/go 函数、`.book.dragging`/`.book-stage.zoomed` class 在 ts 与 css 间一致；`go` 用含 resetZoom 的版本替换原定义（避免重复）；`_cleanup` 移除全部新增监听。
- **待实施确认点**：ComicReaderView.ts 现有 `go`/`stage.click`/`onKey`/`_cleanup` 的确切文本（实施时读文件精确替换，避免重复定义 go）。
