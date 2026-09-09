import { api } from "../lib/ipc";
import type { MediaItem } from "../lib/ipc";
import { esc } from "../lib/escape";
import { icon } from "../lib/icons";

const PREFETCH = 2;

export async function ComicReaderView(it: MediaItem, onExit: () => void): Promise<HTMLElement> {
  const el = document.createElement("div");
  el.className = "comic-reader view-enter";
  // TODO(后端 comic 迁移未完成): MediaItem 已去 path，comic_pages/comic_page 仍收绝对 path。
  // 待后端 comic 命令改收 category_path+title 后同步；暂用相对定位串占位。
  const comicRef = `${it.category_path}/${it.title}`;
  const pages = await api.comicPages(comicRef);
  let idx = 0;
  const cache = new Map<number, string>();
  let mode: "page" | "strip" = "page";

  const load = async (i: number): Promise<string | undefined> => {
    if (i < 0 || i >= pages.length || cache.has(i)) return cache.get(i);
    const url = await api.comicPage(comicRef, pages[i]);
    cache.set(i, url);
    return url;
  };
  const prefetch = () => {
    for (let k = 1; k <= PREFETCH; k++) {
      load(idx + k);
      load(idx - k);
    }
  };

  const renderPage = async () => {
    const url = await load(idx);
    el.querySelector(".stage")!.innerHTML = `<img class="page-img" src="${url ?? ""}"/>`;
    el.querySelector(".pager")!.textContent = `${idx + 1} / ${pages.length}`;
    prefetch();
  };

  const renderStrip = async () => {
    const stage = el.querySelector(".stage")!;
    stage.innerHTML = "";
    for (let i = 0; i < pages.length; i++) {
      const img = document.createElement("img");
      img.className = "strip-img";
      img.loading = "lazy";
      load(i).then((u) => {
        if (u) img.src = u;
      });
      stage.appendChild(img);
    }
  };

  el.innerHTML = `
    <div class="reader-bar glass">
      <button class="icon-text back">${icon("arrowLeft", 16)}<span>返回</span></button>
      <span class="title">${esc(it.title)}</span>
      <span class="pager"></span>
      <button class="toggle">切换：长条</button>
    </div>
    <div class="stage"></div>`;
  const stage = el.querySelector<HTMLElement>(".stage")!;

  el.querySelector<HTMLButtonElement>(".back")!.onclick = onExit;
  el.querySelector<HTMLButtonElement>(".toggle")!.onclick = () => {
    mode = mode === "page" ? "strip" : "page";
    el.querySelector(".toggle")!.textContent = mode === "page" ? "切换：长条" : "切换：翻页";
    stage.className = "stage " + mode;
    if (mode === "page") {
      renderPage();
    } else {
      renderStrip();
    }
  };

  const key = (e: KeyboardEvent) => {
    if (mode !== "page") return;
    if (e.key === "ArrowRight" || e.key === " ") {
      if (idx < pages.length - 1) {
        idx++;
        renderPage();
      }
    }
    if (e.key === "ArrowLeft") {
      if (idx > 0) {
        idx--;
        renderPage();
      }
    }
  };
  document.addEventListener("keydown", key);
  el.addEventListener("comic-reader-detach", () => document.removeEventListener("keydown", key));

  stage.className = "stage page";
  await renderPage();
  return el;
}
