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
  const go = async (d: number) => {
    const ni = idx + d;
    if (ni < 0 || ni >= spreads.length) return;
    idx = ni;
    await render(d > 0 ? "next" : "prev");
  };

  stage.addEventListener("click", (e) => {
    const x = (e as MouseEvent).clientX;
    if (x < window.innerWidth / 2) go(-1);
    else go(1);
  });
  const onKey = (e: KeyboardEvent) => {
    if (e.key === "ArrowLeft") go(-1);
    else if (e.key === "ArrowRight") go(1);
    else if (e.key === "Escape") onExit();
  };
  window.addEventListener("keydown", onKey);
  (el as any)._cleanup = () => window.removeEventListener("keydown", onKey);

  await render();
  return el;
}
