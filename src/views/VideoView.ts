import { api } from "../lib/ipc";
import type { MediaItem } from "../lib/ipc";
import { PosterGrid } from "../components/PosterGrid";

export async function VideoView(onOpen: (it: MediaItem) => void): Promise<HTMLElement> {
  const el = document.createElement("div");
  el.className = "view-enter";
  const items = await api.listMedia("video");
  const cats = ["电影", "动漫", "电视剧"];
  let active = cats[0];
  const render = () => {
    const filtered = items.filter(i => i.category === active);
    el.innerHTML = `<div class="tabs">${cats.map(c =>
      `<span class="tab ${c === active ? "active" : ""}" data-c="${c}">${c}</span>`).join("")}</div>`;
    el.querySelectorAll<HTMLElement>(".tab").forEach(t =>
      t.onclick = () => { active = t.dataset.c!; render(); });
    el.appendChild(PosterGrid(filtered, onOpen));
  };
  render();
  return el;
}
