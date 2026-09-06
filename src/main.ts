import "./styles/theme.css";
import "./styles/animations.css";
import { Sidebar } from "./components/Sidebar";
import { router } from "./lib/router";
import type { Route } from "./lib/router";

const app = document.querySelector<HTMLDivElement>("#app")!;
app.style.display = "flex";
app.appendChild(Sidebar());

const content = document.createElement("main");
content.className = "content";
content.style.flex = "1";
content.style.padding = "20px";
content.style.overflow = "auto";
app.appendChild(content);

async function renderRoute(route: Route) {
  content.innerHTML = `<div class="view-enter"><h1 style="color:var(--text);font-size:20px">${route}</h1></div>`;
}
router.on(renderRoute);
renderRoute(router.current);
