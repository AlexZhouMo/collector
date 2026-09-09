import { api } from "../lib/ipc";
import type { MediaItem } from "../lib/ipc";
import { esc } from "../lib/escape";

export async function ComicView(onOpen: (it: MediaItem) => void): Promise<HTMLElement> {
  const el = document.createElement("div");
  el.className = "view-enter";
  const items = await api.listMedia("comic");
  el.innerHTML = `<h1 style="font-size:20px;margin-bottom:16px">漫画</h1><div class="poster-grid"></div>`;
  const grid = el.querySelector(".poster-grid")!;
  items.forEach((it, i) => {
    const card = document.createElement("div");
    card.className = "poster";
    card.innerHTML = `<div class="poster-img" id="cc-${i}"><div class="poster-ph">加载中…</div></div><div class="poster-title">${esc(it.title)}</div>`;
    card.onclick = () => onOpen(it);
    grid.appendChild(card);
    // TODO(后端 comic 迁移未完成): MediaItem 已去 path，comic_cover 仍收绝对 path。
    // 待后端 comic 命令改收 category_path+title 后同步此处；暂用相对定位串占位。
    api.comicCover(`${it.category_path}/${it.title}`).then(url => {
      const box = card.querySelector(`#cc-${i}`)!;
      box.innerHTML = url ? `<img src="${url}"/>` : `<div class="poster-ph">${esc(it.title)}</div>`;
    }).catch(() => {});
  });
  return el;
}
