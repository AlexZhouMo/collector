import { api } from "../lib/ipc";
import type { MediaItem } from "../lib/ipc";
import { buildVideoTree } from "../lib/videoTree";
import { FolderView } from "../components/FolderView";
import { TreeView } from "../components/TreeView";
import { showContextMenu } from "../components/ContextMenu";
import { openEditDrawer } from "../components/EditDrawer";
import { icon } from "../lib/icons";

type ViewMode = "folder" | "tree";

export async function ComicView(onOpen: (it: MediaItem) => void): Promise<HTMLElement> {
  const el = document.createElement("div");
  el.className = "view-enter video-view comic-tree"; // 复用 video-view 布局；comic-tree 供样式覆盖(文件夹卡与漫画卡等高)
  let items = await api.listMedia("comic");
  let mode: ViewMode = "folder";
  let folderPath = "";
  let treeSelected = "";

  // 刷新：重新拉取漫画列表并重绘
  const refresh = async () => {
    items = await api.listMedia("comic");
    render();
  };

  // 漫画卡右键菜单：编辑（kind=comic）/删除
  const onContext = (it: MediaItem, x: number, y: number) => {
    showContextMenu(x, y, [
      { label: "编辑", onClick: () => openEditDrawer(it, refresh, "", "comic") },
      {
        label: "删除",
        danger: true,
        onClick: async () => {
          if (confirm(`删除「${it.title}」？`)) {
            await api.comicDelete(it.id);
            refresh();
          }
        },
      },
    ]);
  };

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
      ? FolderView(tree, onOpen, onContext, folderPath, (p) => { folderPath = p; }, undefined, false, "comic")
      : TreeView(tree, onOpen, onContext, treeSelected, (p) => { treeSelected = p; }));
  };
  render();
  return el;
}
