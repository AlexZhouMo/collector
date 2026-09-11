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
      <span class="pager"></span>
    </div>
    <div class="book-stage"><div class="book" id="book"></div></div>`;
  el.querySelector<HTMLButtonElement>(".back")!.onclick = onExit;
  const book = el.querySelector<HTMLElement>("#book")!;
  const stage = el.querySelector<HTMLElement>(".book-stage")!;
  const pager = el.querySelector<HTMLElement>(".pager")!;

  const render = async (turningDir: "next" | "prev" | null = null) => {
    const sp = spreads[idx];
    const leftUrl = sp.left ? await load(sp.left.name) : "";
    const rightUrl = sp.right ? await load(sp.right.name) : "";
    book.className = "book" + (sp.single ? " single" : "");
    book.innerHTML =
      (sp.left ? `<div class="leaf left"><img src="${leftUrl}"/></div>` : "") +
      (sp.right ? `<div class="leaf right${turningDir === "next" ? " turn-in" : ""}"><img src="${rightUrl}"/></div>` : "");
    pager.textContent = `${idx + 1} / ${spreads.length}`;
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
