import { convertFileSrc } from "@tauri-apps/api/core";
import type { MediaItem } from "../lib/ipc";

export function PosterGrid(items: MediaItem[], onOpen: (it: MediaItem) => void): HTMLElement {
  const grid = document.createElement("div");
  grid.className = "poster-grid";
  grid.innerHTML = items.map((it, i) => `
    <div class="poster card-hover" data-i="${i}">
      <div class="poster-img">${it.cover_path ? `<img src="${convertFileSrc(it.cover_path)}"/>` : `<div class="poster-ph">${it.title}</div>`}</div>
      <div class="poster-title">${it.title}</div>
    </div>`).join("");
  grid.querySelectorAll<HTMLElement>(".poster").forEach(p =>
    p.onclick = () => onOpen(items[Number(p.dataset.i)]));
  return grid;
}
