import { router } from "../lib/router";
import type { Route } from "../lib/router";
import { icon } from "../lib/icons";

type IconName = "home" | "film" | "book" | "gamepad" | "toolbox" | "settings";
const TOP: [Route, string, IconName][] = [
  ["home", "主页", "home"],
  ["video", "影视", "film"],
  ["comic", "漫画", "book"],
  ["game", "游戏", "gamepad"],
];
const BOTTOM: [Route, string, IconName][] = [
  ["normalize", "工具箱", "toolbox"],
  ["settings", "设置", "settings"],
];

/** 切换折叠：给 #app 加/去 sidebar-collapsed 类。 */
export function toggleSidebar() {
  document.getElementById("app")!.classList.toggle("sidebar-collapsed");
}

export function Sidebar(): HTMLElement {
  const el = document.createElement("aside");
  el.className = "sidebar glass";
  const render = () => {
    const item = ([r, label, ic]: [Route, string, IconName]) =>
      `<div class="nav-item ${router.current === r ? "active" : ""}" data-route="${r}">
        ${icon(ic)}<span class="nav-label">${label}</span>
      </div>`;
    el.innerHTML =
      `<div class="brand">
         <span class="brand-mark">${icon("brand", 18)}<span class="brand-text">COLLECTOR</span></span>
         <button class="collapse-btn" title="折叠侧边栏">${icon("chevronLeft", 16)}</button>
       </div>
       <nav class="nav-top">${TOP.map(item).join("")}</nav>
       <nav class="nav-bottom">${BOTTOM.map(item).join("")}</nav>`;
    el.querySelectorAll<HTMLElement>(".nav-item").forEach(n =>
      n.onclick = () => router.go(n.dataset.route as Route));
    el.querySelector<HTMLButtonElement>(".collapse-btn")!.onclick = toggleSidebar;
  };
  router.on(render);
  render();
  return el;
}
