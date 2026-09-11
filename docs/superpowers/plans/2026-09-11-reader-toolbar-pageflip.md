# 阅读器铺满 + 底部按钮栏 + 真实翻书 实现计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 阅读器中间漫画去 padding 铺满；加底部按钮栏（上一页/下一页/页码/缩小/放大/适屏/返回）；翻页升级为 CSS 3D 单页翻转（临时 flipper 正反面 + 阴影）的真实翻书效果。

**Architecture:** 纯前端。ComicReaderView 加底部工具栏 HTML + 按钮绑定；go(d) 升级为构建临时 flipper 元素做 rotateY 3D 翻转、transitionend 后 render 目标对开；theme.css 加铺满布局、工具栏、flipper 样式。保留上次的缩放/拖动。

**Tech Stack:** vanilla-ts + CSS 3D transform。项目无前端测试框架，靠 tsc + 预览验证。

---

### Task 1: 铺满布局 + 底部按钮栏 + 真实翻书

**Files:**
- Modify: `src/views/ComicReaderView.ts`
- Modify: `src/styles/theme.css`

- [ ] **Step 1: 改 theme.css（铺满 + 工具栏 + flipper）**

在 theme.css 找到 book 相关段，改/加为（`.comic-reader` flex 纵向、`.book-stage` flex:1 去 padding；新增 `.reader-toolbar`；新增 flipper 样式）：

```css
.comic-reader{height:100%;display:flex;flex-direction:column;gap:8px}
.book-stage{flex:1;min-height:0;display:flex;align-items:center;justify-content:center;background:#0a0d16;border-radius:12px;perspective:2400px;overflow:hidden}
.book{display:flex;align-items:center;justify-content:center;gap:0;height:100%;transform-style:preserve-3d;transition:transform .15s ease}
.book.dragging{transition:none}
.book .leaf{height:100%;background:#000}
.book .leaf img{height:100%;width:auto;display:block;object-fit:contain;box-shadow:0 8px 30px rgba(0,0,0,.6)}
.book.single .leaf img{max-width:min(90vw,700px);width:auto}
.book-stage.zoomed{cursor:grab}
.book-stage.zoomed.dragging{cursor:grabbing}
/* 底部按钮栏 */
.reader-toolbar{display:flex;align-items:center;justify-content:center;gap:12px;padding:8px 14px}
.reader-toolbar button{background:var(--glass);border:1px solid var(--border);color:var(--text);border-radius:8px;padding:6px 12px;font-size:13px;cursor:pointer}
.reader-toolbar button:hover{background:var(--glass-strong);border-color:var(--accent)}
.reader-toolbar button:disabled{opacity:.4;cursor:default}
.reader-toolbar .pager{color:var(--text-dim);font-size:12px;min-width:70px;text-align:center}
/* 真实翻书：临时 flipper 绕书脊 rotateY 翻转，正反面 + 阴影 */
.flipper{position:absolute;top:0;height:100%;transform-style:preserve-3d;transform-origin:left center;
  transition:transform .6s cubic-bezier(.4,.15,.2,1);z-index:5}
.flipper.flip-next{transform:rotateY(-180deg)}
.flipper.flip-prev{transform:rotateY(0deg)}
.flipper .face{position:absolute;inset:0;backface-visibility:hidden;background:#000}
.flipper .face img{height:100%;width:auto;display:block;object-fit:contain}
.flipper .back{transform:rotateY(180deg)}
.flipper .shade{position:absolute;inset:0;opacity:0;transition:opacity .6s;
  background:linear-gradient(90deg,rgba(0,0,0,.4),transparent)}
.flipper.flip-next .shade{opacity:1}
```

（保留 `@keyframes leafTurn`（可留着不用或删，YAGNI 留着无害）、`.vol-thumb`、`.loading-box`/spinner 等其它样式。`.book-stage` 需要是 flipper 的定位上下文——加 `position:relative`：把 `.book-stage{...}` 补 `position:relative`。）

- [ ] **Step 2: 改 ComicReaderView.ts（工具栏 HTML + 按钮 + flipper 翻页）**

(a) innerHTML 底部加工具栏，`.book-stage` 保持：

```ts
  el.innerHTML = `
    <div class="reader-bar glass">
      <button class="icon-text back">${icon("arrowLeft", 16)}<span>返回</span></button>
      <span class="title">${esc(mangaTitle)} · ${esc(vol.label)}</span>
    </div>
    <div class="book-stage"><div class="book" id="book"></div></div>
    <div class="reader-toolbar glass">
      <button class="prev">${icon("arrowLeft", 15)}<span>上一页</span></button>
      <span class="pager"></span>
      <button class="next"><span>下一页</span>${icon("arrowRight", 15)}</button>
      <button class="zoom-out">−</button>
      <button class="zoom-in">＋</button>
      <button class="zoom-fit">适屏</button>
    </div>`;
```

（`arrowRight` 图标若 icons.ts 无，用 arrowLeft 翻转或现成的；实施时 grep icons.ts 确认，缺则用文字"›"。pager 从顶栏移到工具栏。返回按钮仍在顶栏。）

(b) 元素引用与按钮绑定（在现有 book/stage 引用处加 pager 指向工具栏、按钮绑定）：

```ts
  const book = el.querySelector<HTMLElement>("#book")!;
  const stage = el.querySelector<HTMLElement>(".book-stage")!;
  const pager = el.querySelector<HTMLElement>(".reader-toolbar .pager")!;
  el.querySelector<HTMLButtonElement>(".back")!.onclick = onExit;
  const updateNav = () => {
    el.querySelector<HTMLButtonElement>(".prev")!.disabled = idx <= 0;
    el.querySelector<HTMLButtonElement>(".next")!.disabled = idx >= spreads.length - 1;
  };
```

(c) 翻书 flipper 逻辑——把现有 `go` 替换为带 3D 翻转的版本（render 保持，但去掉其 turn-in 逻辑，改由 flipper 负责动画）：

```ts
  let flipping = false;
  const render = async () => {
    const sp = spreads[idx];
    const leftUrl = sp.left ? await load(sp.left.name) : "";
    const rightUrl = sp.right ? await load(sp.right.name) : "";
    book.className = "book" + (sp.single ? " single" : "");
    book.innerHTML =
      (sp.left ? `<div class="leaf left"><img src="${leftUrl}"/></div>` : "") +
      (sp.right ? `<div class="leaf right"><img src="${rightUrl}"/></div>` : "");
    pager.textContent = `${idx + 1} / ${spreads.length}`;
    updateNav();
  };

  // 真实翻书：在 stage 上叠临时 flipper，正面=当前右页、背面=目标对开的可见页，绕书脊翻转
  const go = async (d: number) => {
    if (flipping) return;
    const ni = idx + d;
    if (ni < 0 || ni >= spreads.length) return;
    resetZoom();
    const cur = spreads[idx], tgt = spreads[ni];
    // 正面：当前对开的右页（无右页则用左页）；背面：目标对开的“先露出”页
    const frontName = (cur.right ?? cur.left)?.name;
    const backName = d > 0 ? (tgt.left ?? tgt.right)?.name : (tgt.right ?? tgt.left)?.name;
    if (!frontName || !backName) { idx = ni; await render(); return; }
    flipping = true;
    const [frontUrl, backUrl] = await Promise.all([load(frontName), load(backName)]);
    // flipper 宽度取右页实际宽度（用 book 内右 leaf 的宽，回退 book 半宽）
    const rightLeaf = book.querySelector<HTMLElement>(".leaf.right, .leaf.left");
    const w = rightLeaf ? rightLeaf.getBoundingClientRect().width : book.getBoundingClientRect().width / 2;
    const bookRect = book.getBoundingClientRect();
    const stageRect = stage.getBoundingClientRect();
    const fl = document.createElement("div");
    fl.className = "flipper";
    fl.style.width = w + "px";
    // 定位到书脊右侧（book 中线）——书脊在 book 中心
    fl.style.left = (bookRect.left - stageRect.left + bookRect.width / 2) + "px";
    fl.innerHTML =
      `<div class="face front"><img src="${frontUrl}"/></div>` +
      `<div class="face back"><img src="${backUrl}"/></div>` +
      `<div class="shade"></div>`;
    stage.appendChild(fl);
    // 触发翻转（下一页 rotateY→-180，上一页从 -180→0：prev 需先置 -180 再动到 0）
    if (d > 0) {
      requestAnimationFrame(() => fl.classList.add("flip-next"));
    } else {
      fl.style.transform = "rotateY(-180deg)";
      requestAnimationFrame(() => { fl.style.transition = "transform .6s cubic-bezier(.4,.15,.2,1)"; fl.style.transform = "rotateY(0deg)"; });
    }
    const done = () => { fl.remove(); flipping = false; };
    fl.addEventListener("transitionend", async () => { idx = ni; await render(); done(); }, { once: true });
    // 兜底：动画未触发 transitionend 时超时收尾
    setTimeout(async () => { if (flipping) { idx = ni; await render(); done(); } }, 800);
  };
```

（说明：prev 方向的 flipper 定位与正反面按 YAGNI 做近似对称——首版重点是 next 翻页酷炫，prev 可用同一 flipper 反向；若真机 prev 效果不佳，后续微调。flipper 定位在书脊(book 中线)，宽度取一页宽。single 页翻页时 frontName/backName 仍能取到，flipper 覆盖单页区。）

(d) 按钮绑定（在事件绑定区加）：

```ts
  el.querySelector<HTMLButtonElement>(".prev")!.onclick = () => go(-1);
  el.querySelector<HTMLButtonElement>(".next")!.onclick = () => go(1);
  const center = () => ({ px: 0, py: 0 });
  el.querySelector<HTMLButtonElement>(".zoom-in")!.onclick = () => { const c = center(); zoomAt(scale * 1.3, c.px, c.py); };
  el.querySelector<HTMLButtonElement>(".zoom-out")!.onclick = () => { const c = center(); zoomAt(scale / 1.3, c.px, c.py); };
  el.querySelector<HTMLButtonElement>(".zoom-fit")!.onclick = () => resetZoom();
```

（缩放按钮以画面中心为锚 px=py=0，即 stage 中心。现有 wheel/dblclick/mousedown/click/onKey 保留，click 翻页仍在 scale==1 时用 go。）

- [ ] **Step 3: 类型检查**

Run: `npx tsc --noEmit`
Expected: 无输出。修到干净（flipper 元素类型、arrowRight 图标存在性——若 icons.ts 无 arrowRight，用文字或现成图标替代）。

- [ ] **Step 4: 预览验证**

起预览，注入/进入阅读器（预览无 IPC，注入假 book + 手动触发 go 的 flipper 构建验证）：
- inspect `.comic-reader` flex-direction:column、`.book-stage` flex 撑满 + position:relative + overflow hidden。
- `.reader-toolbar` 存在、按钮齐全、pager 在其中。
- 手动 appendChild 一个 `.flipper.flip-next` inspect transform==rotateY(-180deg)、transform-origin left center、shade opacity 过渡。
- 截图看铺满 + 工具栏布局。翻书动画靠真机实际点击验证。

- [ ] **Step 5: 提交**

```bash
git add src/views/ComicReaderView.ts src/styles/theme.css
git commit -m "feat(comic): 阅读器铺满+底部按钮栏+CSS3D真实翻书(flipper绕书脊翻转)

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>"
```

---

## 自查

- **规格覆盖**：铺满（Step 1 comic-reader flex + book-stage flex:1 去 padding）、底部按钮栏无首页（Step 2a/2d 上一页/页码/下一页/缩小/放大/适屏 + 顶栏返回）、真实翻书 flipper 3D（Step 1 flipper 样式 + Step 2c go 构建临时 flipper 绕书脊 rotateY 翻转 + 正反面 + 阴影 + transitionend render）、首末禁用（updateNav）、翻转防重复（flipping 标志）、保留缩放拖动（现有 wheel/dblclick/drag 不动）——规格各条均有对应实现。
- **占位符扫描**：无 TBD；Step 2 给出完整代码。prev 方向翻转与 flipper 定位按 YAGNI 做近似（注明），是明确取舍非占位。
- **一致性**：flipping 标志、go/render/updateNav/zoomAt/resetZoom 一致；pager 从顶栏移到 `.reader-toolbar .pager`（Step 2a 顶栏去掉 pager、工具栏加）；render 去掉 turn-in（改由 flipper 动画）。
- **待实施确认点**：icons.ts 是否有 arrowRight（Step 2a，缺则替代）；`.book-stage` 加 position:relative（Step 1，flipper 定位上下文）；render 内原 turn-in 逻辑删除（Step 2c 用新 render）。
