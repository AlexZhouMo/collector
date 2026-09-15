import { api } from "../lib/ipc";
import type { MediaItem } from "../lib/ipc";
import { buildVideoTree } from "../lib/videoTree";
import { FolderView } from "../components/FolderView";
import { TreeView } from "../components/TreeView";
import { showContextMenu } from "../components/ContextMenu";
import { openEditDrawer } from "../components/EditDrawer";
import { openMoveDialog } from "../components/MoveDialog";
import { icon } from "../lib/icons";

type ViewMode = "folder" | "tree";

export async function GameView(onOpen: (it: MediaItem) => void): Promise<HTMLElement> {
  const el = document.createElement("div");
  // 复用 comic-tree 的一级方形/二级长方形与卡片等高样式；额外加 game-tree 以备将来定制。
  el.className = "view-enter video-view game-tree comic-tree";
  let items = await api.listMedia("game");
  let mode: ViewMode = "folder";
  let folderPath = "";
  let treeSelected = "";

  // 刷新：重新拉取游戏列表并重绘
  const refresh = async () => {
    items = await api.listMedia("game");
    render();
  };

  // 游戏卡右键菜单：编辑（kind=game）/移动/删除
  const onContext = (it: MediaItem, x: number, y: number) => {
    showContextMenu(x, y, [
      { label: "编辑", onClick: () => openEditDrawer(it, refresh, "", "game") },
      {
        label: "移动",
        onClick: () => {
          const tree = buildVideoTree("游戏", items);
          openMoveDialog(it, tree, "game", refresh);
        },
      },
      {
        label: "删除",
        danger: true,
        onClick: async () => {
          if (confirm(`删除「${it.title}」？`)) {
            await api.gameDelete(it.id);
            refresh();
          }
        },
      },
    ]);
  };

  const render = () => {
    // 游戏树：根 name="游戏"（面包屑首级显示"游戏"、点击回根），category_path 即完整层级路径
    const tree = buildVideoTree("游戏", items);
    el.innerHTML = `
      <div class="video-bar">
        <div class="tabs"></div>
        <div class="video-bar-right">
          <button class="add-video-btn">+ 新增游戏</button>
          <div class="view-toggle">
            <button class="vt-btn ${mode === "folder" ? "active" : ""}" data-mode="folder" title="文件夹视图">${icon("folder", 16)}</button>
            <button class="vt-btn ${mode === "tree" ? "active" : ""}" data-mode="tree" title="树形视图">${icon("tree", 16)}</button>
          </div>
        </div>
      </div>
      <div class="video-body"></div>`;
    el.querySelectorAll<HTMLButtonElement>(".vt-btn").forEach(b =>
      b.onclick = () => { mode = b.dataset.mode as ViewMode; render(); });
    const addBtn = el.querySelector<HTMLButtonElement>(".add-video-btn")!;
    addBtn.onclick = () => openEditDrawer(null, refresh, "", "game", folderPath);
    const updateAddBtn = () => {
      addBtn.style.display = (mode === "folder" && folderPath !== "") ? "" : "none";
    };
    updateAddBtn();
    const body = el.querySelector<HTMLElement>(".video-body")!;
    body.appendChild(mode === "folder"
      ? FolderView(tree, onOpen, onContext, folderPath, (p) => { folderPath = p; updateAddBtn(); },
          () => { refresh(); }, true, "game")
      : TreeView(tree, onOpen, onContext, treeSelected, (p) => { treeSelected = p; }));
  };
  render();
  return el;
}
