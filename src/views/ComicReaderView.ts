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
  const pages = await api.comicPages(vol.zip_path);
  const spreads = buildSpreads(pages);
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

  // 真实翻书：在 stage 上叠临时 flipper，正面=当前右页、背面=目标对开的可见页，绕书脊翻转
  let flipping = false;
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
    // flipper 作为 book 子元素、相对 book 定位（覆盖右半页/单页），与书页天然对齐，
    // 不依赖运行时坐标计算——避免图片异步布局导致的错位。
    const fl = document.createElement("div");
    fl.className = "flipper";
    fl.innerHTML =
      `<div class="face front"><img src="${frontUrl}"/></div>` +
      `<div class="face back"><img src="${backUrl}"/></div>` +
      `<div class="shade"></div>`;
    book.appendChild(fl);
    // 触发翻转：先让初始态(rotateY 0 / -180)绘制一帧(双 rAF，WebKit 才可靠 transition)，
    // 再切到目标角度，避免 none→rotateY 跳变导致「一闪而过」。
    const raf2 = (cb: () => void) => requestAnimationFrame(() => requestAnimationFrame(cb));
    if (d > 0) {
      raf2(() => fl.classList.add("flip-next"));
    } else {
      fl.style.transform = "rotateY(-180deg)";
      raf2(() => { fl.style.transform = "rotateY(0deg)"; });
    }
    const done = () => { fl.remove(); flipping = false; };
    fl.addEventListener("transitionend", async () => { idx = ni; await render(); done(); }, { once: true });
    // 兜底：动画未触发 transitionend 时超时收尾
    setTimeout(async () => { if (flipping) { idx = ni; await render(); done(); } }, 800);
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
    window.removeEventListener("keydown", onKey);
    window.removeEventListener("mousemove", onMove);
    window.removeEventListener("mouseup", onUp);
  };

  await render();
  return el;
}
