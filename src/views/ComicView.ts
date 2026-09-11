import { api } from "../lib/ipc";
import type { MediaItem } from "../lib/ipc";
import { buildVideoTree } from "../lib/videoTree";
import { FolderView } from "../components/FolderView";
import { TreeView } from "../components/TreeView";
import { icon } from "../lib/icons";

type ViewMode = "folder" | "tree";

export async function ComicView(onOpen: (it: MediaItem) => void): Promise<HTMLElement> {
  const el = document.createElement("div");
  el.className = "view-enter video-view comic-tree"; // 复用 video-view 布局；comic-tree 供样式覆盖(文件夹卡与漫画卡等高)
  const items = await api.listMedia("comic");
  let mode: ViewMode = "folder";
  let folderPath = "";
  let treeSelected = "";

  const render = () => {
    // 漫画树：根 name="漫画"（面包屑首级显示"漫画"、点击回根），category_path 即完整层级路径
    const tree = buildVideoTree("漫画", items);
    el.innerHTML = `
      <div class="video-bar">
        <div class="tabs"></div>
        <div class="video-bar-right">
          <div class="view-toggle">
            <button class="vt-btn ${mode === "folder" ? "active" : ""}" data-mode="folder" title="文件夹视图">${icon("folder", 16)}</button>
            <button class="vt-btn ${mode === "tree" ? "active" : ""}" data-mode="tree" title="树形视图">${icon("tree", 16)}</button>
          </div>
        </div>
      </div>
      <div class="video-body"></div>`;
    el.querySelectorAll<HTMLButtonElement>(".vt-btn").forEach(b =>
      b.onclick = () => { mode = b.dataset.mode as ViewMode; render(); });
    const body = el.querySelector<HTMLElement>(".video-body")!;
    body.appendChild(mode === "folder"
      ? FolderView(tree, onOpen, undefined, folderPath, (p) => { folderPath = p; }, undefined, false)
      : TreeView(tree, onOpen, undefined, treeSelected, (p) => { treeSelected = p; }));
  };
  render();
  return el;
}
