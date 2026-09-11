import { convertFileSrc } from "@tauri-apps/api/core";
import { api } from "../lib/ipc";
import type { MediaItem } from "../lib/ipc";
import { esc } from "../lib/escape";

export async function ComicView(onOpen: (it: MediaItem) => void): Promise<HTMLElement> {
  const el = document.createElement("div");
  el.className = "view-enter";
  const items = await api.listMedia("comic");
  el.innerHTML = `<h1 style="font-size:20px;margin-bottom:16px">漫画</h1><div class="poster-grid"></div>`;
  const grid = el.querySelector(".poster-grid")!;
  items.forEach((it) => {
    const card = document.createElement("div");
    card.className = "poster";
    const cover = it.cover_path
      ? `<img src="${convertFileSrc(it.cover_path)}"/>`
      : `<div class="poster-ph">${esc(it.title)}</div>`;
    card.innerHTML = `<div class="poster-img">${cover}</div><div class="poster-title">${esc(it.title)}</div>`;
    card.onclick = () => onOpen(it);
    grid.appendChild(card);
  });
  return el;
}
