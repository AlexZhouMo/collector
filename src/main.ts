import "./styles/theme.css";
import "./styles/animations.css";
import { Sidebar } from "./components/Sidebar";
import { router } from "./lib/router";
import type { Route } from "./lib/router";
import { VideoView } from "./views/VideoView";
import { ComicView } from "./views/ComicView";
import { SettingsView } from "./views/SettingsView";
import { ComicReaderView } from "./views/ComicReaderView";
import { PlayerView } from "./views/PlayerView";
import type { MediaItem } from "./lib/ipc";

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
  content.innerHTML = "";
  let view: HTMLElement;
  switch (route) {
    case "video": view = await VideoView((it) => openPlayer(it)); break;
    case "comic": view = await ComicView((it) => openComicReader(it)); break;
    case "settings": view = await SettingsView(); break;
    default: view = document.createElement("div"); view.className = "view-enter";
             view.innerHTML = `<h1 style="font-size:20px">${route}（后续阶段）</h1>`;
  }
  content.appendChild(view);
}
router.on(renderRoute);
renderRoute(router.current);

async function openComicReader(it: MediaItem) {
  const prev = content.querySelector(".comic-reader");
  prev?.dispatchEvent(new Event("comic-reader-detach"));
  content.innerHTML = "";
  content.appendChild(await ComicReaderView(it, () => renderRoute("comic")));
}

async function openPlayer(it: MediaItem) {
  const prev = content.querySelector(".player-view");
  prev?.dispatchEvent(new Event("player-detach"));
  content.innerHTML = "";
  content.appendChild(await PlayerView(it, () => renderRoute("video")));
}
