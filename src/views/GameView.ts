import { convertFileSrc } from "@tauri-apps/api/core";
import { api } from "../lib/ipc";

export async function GameView(): Promise<HTMLElement> {
  const el = document.createElement("div");
  el.className = "view-enter";
  const items = await api.listMedia("game");
  el.innerHTML = `<h1 style="font-size:20px;margin-bottom:16px">游戏</h1>
    <div class="game-grid"></div>`;
  const grid = el.querySelector(".game-grid")!;
  grid.innerHTML = items.map((it, i) => `
    <div class="game-card glass card-hover ${it.platform_ok ? "" : "disabled"}" data-i="${i}">
      <div class="game-cover">${it.cover_path ? `<img src="${convertFileSrc(it.cover_path)}"/>` : ""}</div>
      <div class="game-info">
        <div class="game-title">${it.title}</div>
        <div class="game-desc">${it.description ?? ""}</div>
        ${it.platform_ok ? "" : `<div class="game-na">本平台不可用</div>`}
      </div>
    </div>`).join("");
  grid.querySelectorAll<HTMLElement>(".game-card").forEach(c => {
    const it = items[Number(c.dataset.i)];
    c.ondblclick = () => { if (it.platform_ok && it.exec_path) api.launchGame(it.id, it.exec_path); };
  });
  return el;
}
