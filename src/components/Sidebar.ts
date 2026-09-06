import { router } from "../lib/router";
import type { Route } from "../lib/router";

const TOP: [Route, string][] = [["video","▶ 视频"],["comic","▤ 漫画"],["game","◉ 游戏"]];
const BOTTOM: [Route, string][] = [["normalize","⚙ 标准化"],["settings","⚙ 设置"]];

export function Sidebar(): HTMLElement {
  const el = document.createElement("aside");
  el.className = "sidebar glass";
  const render = () => {
    const item = ([r,label]: [Route,string]) =>
      `<div class="nav-item ${router.current===r?"active":""}" data-route="${r}">${label}</div>`;
    el.innerHTML =
      `<div class="brand">◈ COLLECTOR</div>
       <nav class="nav-top">${TOP.map(item).join("")}</nav>
       <nav class="nav-bottom">${BOTTOM.map(item).join("")}</nav>`;
    el.querySelectorAll<HTMLElement>(".nav-item").forEach(n =>
      n.onclick = () => router.go(n.dataset.route as Route));
  };
  router.on(render);
  render();
  return el;
}
