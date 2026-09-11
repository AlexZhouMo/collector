# 纯 CSS 翻书深度打磨 实现计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 纯 CSS 提升翻书观感：阴影光影层次、相邻页联动（翻页前底层垫目标页掀开揭示）、缓动手感、弯曲错觉（高光带+阴影）。

**Architecture:** theme.css 加多层 shade/glare、书脊中缝阴影、keyframes 驱动阴影随翻转变化、缓动与时长调整；ComicReaderView go() 改为翻页前底层先渲染目标对开、flipper 叠其上翻转揭示、flipper 加 glare 层、transitionend 判 propertyName=transform 才收尾。纯前端，无后端。

**Tech Stack:** vanilla-ts + CSS 3D transform/animation。真机验证观感（预览 Chromium 与 WKWebView 表现不同）。

---

### Task 1: 翻书阴影光影 + 相邻页联动 + 缓动 + 弯曲错觉

**Files:**
- Modify: `src/styles/theme.css`
- Modify: `src/views/ComicReaderView.ts`

- [ ] **Step 1: 改 theme.css（阴影/高光/书脊/缓动）**

在 flipper 相关段（`.book .flipper` 起）改/加：

```css
/* flipper 翻转：更从容的重量感缓动，约 0.72s */
.book .flipper{position:absolute;top:0;bottom:0;left:50%;right:0;transform:rotateY(0deg);
  transform-style:preserve-3d;transform-origin:left center;
  transition:transform .72s cubic-bezier(.36,.05,.2,1);z-index:6}
.book.single .flipper{left:0}
.flipper.flip-next{transform:rotateY(-180deg)}
.flipper .face{position:absolute;inset:0;backface-visibility:hidden;background:#000;overflow:hidden;display:flex;align-items:center;justify-content:center}
.flipper .face img{max-width:100%;max-height:100%;width:auto;height:auto;display:block;object-fit:contain}
.flipper .back{transform:rotateY(180deg)}
/* 阴影层：翻转时随进度渐深（书脊侧深、外缘浅），用 keyframes 与翻转同步 */
.flipper .shade{position:absolute;inset:0;pointer-events:none;opacity:0;
  background:linear-gradient(90deg,rgba(0,0,0,.55),rgba(0,0,0,.15) 40%,transparent)}
.flipper.flip-next .shade{animation:flipShade .72s cubic-bezier(.36,.05,.2,1) forwards}
@keyframes flipShade{0%{opacity:0}30%{opacity:.9}70%{opacity:.7}100%{opacity:0}}
/* 高光带：翻转时一条亮带扫过，模拟纸面反光弯曲错觉 */
.flipper .glare{position:absolute;inset:0;pointer-events:none;opacity:0;
  background:linear-gradient(105deg,transparent 30%,rgba(255,255,255,.35) 50%,transparent 70%)}
.flipper.flip-next .glare{animation:flipGlare .72s cubic-bezier(.36,.05,.2,1) forwards}
@keyframes flipGlare{0%{opacity:0;background-position:-60% 0}40%{opacity:.8}100%{opacity:0;background-position:60% 0}}
/* 书脊中缝阴影：双页对开中间常驻暗影（single 时隐藏） */
.book::after{content:"";position:absolute;top:0;bottom:0;left:50%;width:24px;transform:translateX(-50%);
  pointer-events:none;background:linear-gradient(90deg,transparent,rgba(0,0,0,.35),transparent);z-index:4}
.book.single::after{display:none}
```

> 说明：glare 用 `background-position` 动画需 `background-size` 放大——给 `.flipper .glare` 加
> `background-size:220% 100%`。shade/glare 用 `animation ... forwards` 与 flipper 的 transform
> transition 同时长同缓动，视觉同步。`.book::after` 需 `.book` 有 position:relative（已有）。

补 glare 的 background-size：把 `.flipper .glare` 行改为含 `background-size:220% 100%`。

- [ ] **Step 2: 改 ComicReaderView.ts go()（相邻页联动 + glare 层 + 收尾判定）**

把 go() 从 flipper 构建到收尾整段替换为——**先把底层 book 渲染为目标对开**（idx 先设为 ni 并 render），
再叠 flipper（正面=原当前页、背面=原当前页对应的目标侧，实际是"盖住已换成目标页的底层、翻走露出它"）：

```ts
  let flipping = false;
  const go = async (d: number) => {
    if (flipping) return;
    const ni = idx + d;
    if (ni < 0 || ni >= spreads.length) return;
    resetZoom();
    const cur = spreads[idx];
    // 翻走的页正面 = 当前对开右页（无则左页）
    const frontName = (cur.right ?? cur.left)?.name;
    if (!frontName) { idx = ni; await render(); return; }
    flipping = true;
    const frontUrl = await load(frontName);
    // 相邻页联动：底层先渲染为目标对开（掀开当前页即露出下一页）
    idx = ni;
    await render();
    // 叠 flipper：正面=翻走的原页；背面留空/同底层（掀开即见底层目标页），加 shade+glare
    const fl = document.createElement("div");
    fl.className = "flipper";
    fl.innerHTML =
      `<div class="face front"><img src="${frontUrl}"/></div>` +
      `<div class="face back"></div>` +
      `<div class="shade"></div>` +
      `<div class="glare"></div>`;
    book.appendChild(fl);
    const raf2 = (cb: () => void) => requestAnimationFrame(() => requestAnimationFrame(cb));
    if (d > 0) {
      raf2(() => fl.classList.add("flip-next"));
    } else {
      fl.style.transform = "rotateY(-180deg)";
      raf2(() => { fl.style.transform = "rotateY(0deg)"; });
    }
    const done = () => { fl.remove(); flipping = false; };
    // 仅 transform 的 transition 结束才收尾（避免 shade/glare 动画干扰）
    fl.addEventListener("transitionend", (e) => { if ((e as TransitionEvent).propertyName === "transform") done(); });
    setTimeout(() => { if (flipping) done(); }, 900);
  };
```

> 变化点：(1) 底层先 render 目标对开（idx=ni; render）→ flipper 只需正面（翻走的原页），背面空
> （翻走后露出底层已换的目标页），联动更自然。(2) flipper 加 glare 层。(3) transitionend 判
> propertyName==="transform" 才 done（不再在收尾里重复 render——底层已是目标页；只 fl.remove）。
> (4) done 不再 idx=ni/render（已提前做）。pager/updateNav 已在 render 内更新。

- [ ] **Step 3: 类型检查**

Run: `npx tsc --noEmit`
Expected: 无输出。修到干净（TransitionEvent 类型、无未用变量——注意原 backName/tgt 现可能不再用，删除未用变量）。

- [ ] **Step 4: 预览结构验证（辅助）**

起预览注入 flipper，inspect：`.flipper .glare`/`.shade` 存在、`.book::after`（书脊缝）存在、flipper transition-duration≈0.72s。观感靠真机（预览 Chromium 与 WKWebView 不同）。截图看静态层次。

- [ ] **Step 5: 提交**

```bash
git add src/styles/theme.css src/views/ComicReaderView.ts
git commit -m "feat(comic): 翻书打磨—多层阴影/高光带弯曲错觉+相邻页联动掀开+重量感缓动

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>"
```

---

## 自查

- **规格覆盖**：阴影光影层次（Step 1 shade 多层渐变+keyframes、书脊 book::after 中缝）、弯曲错觉（Step 1 glare 高光带+keyframes）、缓动手感（Step 1 0.72s + 重量感 cubic-bezier）、相邻页联动（Step 2 底层先 render 目标对开、flipper 掀开揭示）、收尾判定（Step 2 transitionend propertyName===transform）——规格各条均有对应实现。
- **占位符扫描**：无 TBD；Step 1/2 给出完整代码。glare background-size 在 Step 1 末尾补充说明，非占位。
- **一致性**：flipping 标志、go/render/done、flip-next class、shade/glare/book::after 在 ts 与 css 间一致；缩放拖动/工具栏/双页 fit 不动；初始 transform:rotateY(0)+双 rAF（上次修复）保留。
- **待实施确认点**：go() 里原 backName/tgt 变量删除避免 unused（Step 3 tsc）；`.flipper .glare` 补 background-size:220%（Step 1）；`.book::after` 依赖 book position:relative（已有）。
