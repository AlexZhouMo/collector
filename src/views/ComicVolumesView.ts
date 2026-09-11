import { api } from "../lib/ipc";
import type { MediaItem, VolumeInfo } from "../lib/ipc";
import { esc } from "../lib/escape";
import { icon } from "../lib/icons";

export async function ComicVolumesView(
  it: MediaItem,
  onOpenVol: (vol: VolumeInfo, mangaTitle: string) => void,
  onBack: () => void,
): Promise<HTMLElement> {
  const el = document.createElement("div");
  el.className = "view-enter";
  const vols = await api.comicVolumes(it.category_path, it.title);
  el.innerHTML = `
    <div class="reader-bar glass">
      <button class="icon-text back">${icon("arrowLeft", 16)}<span>返回</span></button>
      <span class="title">${esc(it.title)}</span>
    </div>
    <div class="poster-grid"></div>`;
  el.querySelector<HTMLButtonElement>(".back")!.onclick = onBack;
  const grid = el.querySelector(".poster-grid")!;
  vols.forEach((v) => {
    const card = document.createElement("div");
    card.className = "poster";
    card.innerHTML = `<div class="poster-img vol-thumb">${icon("book", 48)}</div><div class="poster-title">${esc(v.label)}</div>`;
    card.onclick = () => onOpenVol(v, it.title);
    grid.appendChild(card);
    const thumb = card.querySelector<HTMLElement>(".vol-thumb")!;
    api.comicVolumeCover(v.zip_path).then((url) => {
      if (url) thumb.innerHTML = `<img src="${url}" alt="${esc(v.label)}"/>`;
    }).catch(() => {});
  });
  if (vols.length === 0) {
    grid.innerHTML = `<div class="poster-ph">暂无分卷</div>`;
  }
  return el;
}
