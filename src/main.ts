import "./styles/theme.css";
import "./styles/animations.css";
import { Sidebar, toggleSidebar } from "./components/Sidebar";
import { icon } from "./lib/icons";
import { router } from "./lib/router";
import type { Route } from "./lib/router";
import { HomeView } from "./views/HomeView";
import { VideoView } from "./views/VideoView";
import { ComicView } from "./views/ComicView";
import { ComicVolumesView } from "./views/ComicVolumesView";
import { SettingsView } from "./views/SettingsView";
import { ComicReaderView } from "./views/ComicReaderView";
import { PlayerView } from "./views/PlayerView";
import { GameView } from "./views/GameView";
import { NormalizeView } from "./views/NormalizeView";
import { api } from "./lib/ipc";
import type { MediaItem, VolumeInfo } from "./lib/ipc";
import { listen } from "@tauri-apps/api/event";
import { applyProgress } from "./lib/normalizeStore";

const app = document.querySelector<HTMLDivElement>("#app")!;
app.style.display = "flex";

// 全局禁用 WebView 默认右键菜单（重新加载/检查/复制图片等）。
// 视频条目 CRUD 菜单在元素 oncontextmenu 里主动弹出，不依赖系统菜单，不受影响。
document.addEventListener("contextmenu", (e) => e.preventDefault());

app.appendChild(Sidebar());

const expandBtn = document.createElement("button");
expandBtn.className = "expand-btn glass";
expandBtn.title = "展开侧边栏";
expandBtn.innerHTML = icon("chevronRight", 16);
expandBtn.onclick = toggleSidebar;
app.appendChild(expandBtn);

const content = document.createElement("main");
content.className = "content";
app.appendChild(content);

async function renderRoute(route: Route, videoInitial?: { category: string; folderPath: string }, comicFolder?: string) {
  cleanupContent();
  content.innerHTML = "";
  let view: HTMLElement;
  switch (route) {
    case "home": view = await HomeView(); break;
    case "video": view = await VideoView((it) => openPlayer(it), videoInitial); break;
    case "comic": view = await ComicView((it) => openComicVolumes(it), comicFolder); break;
    case "game": view = await GameView((it) => api.launchGame(it.category_path, it.title)); break;
    case "normalize": view = NormalizeView(); break;
    case "settings": view = await SettingsView(); break;
    default: view = document.createElement("div"); view.className = "view-enter";
             view.innerHTML = `<h1 style="font-size:20px">${route}（后续阶段）</h1>`;
  }
  content.appendChild(view);
}
router.on(renderRoute);
renderRoute(router.current);

// 工具箱长任务的进度事件：app 生命周期内注册一次、常驻不 unlisten。
// 只更新 normalizeStore（不依赖工具箱视图是否在场），故切菜单后仍能收进度、切回可续显。
listen("subtitle-progress", (e) => applyProgress("subtitle", e.payload));
listen("comic-archive-progress", (e) => applyProgress("comic", e.payload));

// 清理当前 content 内视图挂的 keydown 等监听（视图在自身 DOM 上存 _cleanup）。
function cleanupContent() {
  content.querySelectorAll<HTMLElement>("*").forEach((n) => {
    const c = (n as any)._cleanup;
    if (typeof c === "function") c();
  });
}

// 挂载漫画子视图：先清理旧视图监听，再替换内容。
async function mountComic(view: HTMLElement) {
  cleanupContent();
  content.innerHTML = "";
  content.appendChild(view);
}

// 立即显示加载占位：打开漫画/卷需 await 后端(列卷、列页+首图)，
// 在视图构造完成前先给反馈，避免用户感到"点了没反应"。
function showLoading(text: string) {
  cleanupContent();
  content.innerHTML = `<div class="view-enter" style="height:100%;display:flex;align-items:center;justify-content:center">
    <div class="loading-box"><div class="spinner"></div><div class="player-loading">${text}</div></div>
  </div>`;
}

// 加载失败时替换掉加载占位：显示原因 + 返回（可选"前往设置"），避免永远停在转圈。
function showError(title: string, err: unknown, onBack: () => void) {
  cleanupContent();
  content.innerHTML = "";
  const box = document.createElement("div");
  box.className = "view-enter";
  box.style.cssText = "height:100%;display:flex;align-items:center;justify-content:center";
  const inner = document.createElement("div");
  inner.className = "loading-box";
  const h = document.createElement("div");
  h.textContent = title;
  h.style.fontSize = "15px";
  const msg = document.createElement("div");
  msg.className = "player-loading";
  msg.textContent = typeof err === "string" ? err : (err as any)?.message ?? String(err);
  const btns = document.createElement("div");
  btns.style.cssText = "display:flex;gap:10px";
  const back = document.createElement("button");
  back.className = "icon-text";
  back.textContent = "返回";
  back.onclick = onBack;
  const settings = document.createElement("button");
  settings.className = "icon-text";
  settings.textContent = "前往设置";
  settings.onclick = () => router.go("settings");
  btns.append(back, settings);
  inner.append(h, msg, btns);
  box.appendChild(inner);
  content.appendChild(box);
}

async function openComicVolumes(it: MediaItem) {
  showLoading("加载卷列表…");
  try {
    await mountComic(await ComicVolumesView(
      it,
      (vol, title) => openComicReader(vol, title, it),
      // 返回到该漫画所在的上一级目录，而不是漫画根目录
      () => renderRoute("comic", undefined, it.category_path),
    ));
  } catch (e) {
    showError("卷列表加载失败", e, () => renderRoute("comic", undefined, it.category_path));
  }
}

async function openComicReader(vol: VolumeInfo, mangaTitle: string, it: MediaItem) {
  showLoading(`正在打开 ${vol.label}…`);
  try {
    await mountComic(await ComicReaderView(vol, mangaTitle, () => openComicVolumes(it)));
  } catch (e) {
    showError(`${vol.label} 打开失败`, e, () => openComicVolumes(it));
  }
}

async function openPlayer(it: MediaItem) {
  const prev = content.querySelector(".player-view");
  prev?.dispatchEvent(new Event("player-detach"));
  content.innerHTML = "";
  // 返回时定位到该视频所在目录（category_path 即其挂载目录），不回分类根
  content.appendChild(await PlayerView(it, () =>
    renderRoute("video", { category: it.category, folderPath: it.category_path })));
}
