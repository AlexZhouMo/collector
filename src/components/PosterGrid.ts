import { convertFileSrc } from "@tauri-apps/api/core";
import type { MediaItem } from "../lib/ipc";
import { esc } from "../lib/escape";
import { displayTitle } from "../lib/displayTitle";

export function PosterGrid(
  items: MediaItem[],
  onOpen: (it: MediaItem) => void,
  onContext?: (it: MediaItem, x: number, y: number) => void
): HTMLElement {
  const grid = document.createElement("div");
  grid.className = "poster-grid";
  grid.innerHTML = items.map((it, i) => `
    <div class="poster${it.playable === false ? " disabled" : ""}" data-i="${i}">
      <div class="poster-img">${it.cover_path ? `<img src="${convertFileSrc(it.cover_path)}"/>` : `<div class="poster-ph">${esc(displayTitle(it.title))}</div>`}</div>
      <div class="poster-title">${esc(displayTitle(it.title))}</div>
    </div>`).join("");
  grid.querySelectorAll<HTMLElement>(".poster").forEach(p => {
    p.onclick = () => { const it = items[Number(p.dataset.i)]; if (it.playable === false) return; onOpen(it); };
    p.oncontextmenu = (e) => { e.preventDefault(); onContext?.(items[Number(p.dataset.i)], e.clientX, e.clientY); };
  });
  return grid;
}
