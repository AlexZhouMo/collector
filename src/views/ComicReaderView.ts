import { api } from "../lib/ipc";
import type { VolumeInfo } from "../lib/ipc";
import { esc } from "../lib/escape";
import { icon } from "../lib/icons";
import { buildSpreads } from "../lib/spreads";

export async function ComicReaderView(
  vol: VolumeInfo,
  mangaTitle: string,
  onExit: () => void,
): Promise<HTMLElement> {
  const el = document.createElement("div");
  el.className = "view-enter comic-reader";
  // 阶段一：只取页名，全部按竖单页秒开（w/h=0 → buildSpreads 视作竖页）
  const names = await api.comicPageNames(vol.zip_path);
  let pages = names.map((name) => ({ name, w: 0, h: 0 }));
  let spreads = buildSpreads(pages);
  let idx = 0;
  const cache = new Map<string, string>();
  const load = async (name: string): Promise<string> => {
    if (cache.has(name)) return cache.get(name)!;
    const url = await api.comicPage(vol.zip_path, name);
    cache.set(name, url);
    return url;
  };

  el.innerHTML = `
    <div class="reader-bar glass">
      <button class="icon-text back">${icon("arrowLeft", 16)}<span>返回</span></button>
      <span class="title">${esc(mangaTitle)} · ${esc(vol.label)}</span>
    </div>
    <div class="book-stage"><div class="book" id="book"></div></div>
    <div class="reader-toolbar glass">
      <button class="prev">${icon("arrowLeft", 15)}<span>上一页</span></button>
      <span class="pager"></span>
      <button class="next"><span>下一页</span><span aria-hidden="true">›</span></button>
      <button class="zoom-out">−</button>
      <button class="zoom-in">＋</button>
      <button class="zoom-fit">适屏</button>
    </div>`;
  el.querySelector<HTMLButtonElement>(".back")!.onclick = onExit;
  const book = el.querySelector<HTMLElement>("#book")!;
  const stage = el.querySelector<HTMLElement>(".book-stage")!;
  const pager = el.querySelector<HTMLElement>(".reader-toolbar .pager")!;
  const updateNav = () => {
    el.querySelector<HTMLButtonElement>(".prev")!.disabled = idx <= 0;
    el.querySelector<HTMLButtonElement>(".next")!.disabled = idx >= spreads.length - 1;
  };

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
  // 缩放/平移状态：transform 作用于整个 .book（双页整体）
  let scale = 1, tx = 0, ty = 0;
  let dragging = false, dragStartX = 0, dragStartY = 0, txStart = 0, tyStart = 0;
  const MIN = 1, MAX = 4;
  const applyTransform = () => {
    book.style.transform = `translate(${tx}px, ${ty}px) scale(${scale})`;
    stage.classList.toggle("zoomed", scale > 1);
  };
  const resetZoom = () => { scale = 1; tx = 0; ty = 0; applyTransform(); };

  let closed = false;
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
    // 叠 flipper：正面=翻走的原页；背面留空（掀开即见底层目标页），加 shade+glare
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
    const { px, py } = rectCenter(e as unknown as MouseEvent);
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

  el.querySelector<HTMLButtonElement>(".prev")!.onclick = () => go(-1);
  el.querySelector<HTMLButtonElement>(".next")!.onclick = () => go(1);
  const center = () => ({ px: 0, py: 0 });
  el.querySelector<HTMLButtonElement>(".zoom-in")!.onclick = () => { const c = center(); zoomAt(scale * 1.3, c.px, c.py); };
  el.querySelector<HTMLButtonElement>(".zoom-out")!.onclick = () => { const c = center(); zoomAt(scale / 1.3, c.px, c.py); };
  el.querySelector<HTMLButtonElement>(".zoom-fit")!.onclick = () => resetZoom();

  const onKey = (e: KeyboardEvent) => {
    if (e.key === "ArrowLeft") go(-1);
    else if (e.key === "ArrowRight") go(1);
    else if (e.key === "Escape") onExit();
  };
  window.addEventListener("keydown", onKey);
  (el as any)._cleanup = () => {
    closed = true;
    window.removeEventListener("keydown", onKey);
    window.removeEventListener("mousemove", onMove);
    window.removeEventListener("mouseup", onUp);
  };

  await render();

  // 阶段二：后台取带尺寸的页表，重算对开分组并就地校正。
  // 用「当前显示的页名」重新定位 idx，避免重排后页码跳动；
  // 若正在翻页动画中，推迟到动画结束再应用。
  (async () => {
    let dimsPages;
    try {
      dimsPages = await api.comicPages(vol.zip_path);
    } catch {
      return; // 尺寸取失败：保持竖单页分组，不影响阅读
    }
    if (closed) return;
    const applyDims = () => {
      if (closed) return;
      // 记录当前对开首个页名，用于重排后重定位
      const anchorName = (spreads[idx]?.left ?? spreads[idx]?.right)?.name;
      pages = dimsPages;
      spreads = buildSpreads(pages);
      // 重定位 idx 到含 anchorName 的对开
      if (anchorName) {
        const ni = spreads.findIndex(
          (sp) => sp.left?.name === anchorName || sp.right?.name === anchorName,
        );
        if (ni >= 0) idx = ni;
      }
      if (idx >= spreads.length) idx = Math.max(0, spreads.length - 1);
      render();
    };
    if (flipping) {
      const iv = window.setInterval(() => {
        if (closed) { clearInterval(iv); return; }
        if (!flipping) { clearInterval(iv); applyDims(); }
      }, 100);
    } else {
      applyDims();
    }
  })();

  return el;
}
