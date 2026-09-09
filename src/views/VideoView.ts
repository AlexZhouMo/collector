import { api } from "../lib/ipc";
import type { MediaItem } from "../lib/ipc";
import { buildVideoTree, collectFolderPaths } from "../lib/videoTree";
import { FolderView } from "../components/FolderView";
import { TreeView } from "../components/TreeView";
import { showContextMenu } from "../components/ContextMenu";
import { openEditDrawer } from "../components/EditDrawer";
import { icon } from "../lib/icons";

type ViewMode = "folder" | "tree";

export async function VideoView(
  onOpen: (it: MediaItem) => void,
  initial?: { category: string; folderPath: string }
): Promise<HTMLElement> {
  const el = document.createElement("div");
  el.className = "view-enter video-view";
  let items = await api.listMedia("video");
  const cats = ["电影", "动漫", "剧集"];
  let activeCat = initial && cats.includes(initial.category) ? initial.category : cats[0];
  let mode: ViewMode = "folder";
  // 当前浏览位置（提升为视图状态，refresh 重建时保留，避免保存后跳回分类根）
  // initial 提供时（如从播放器返回）定位到该视频所在目录，否则用分类根（空串）
  // 节点 path 语义为「分类内相对路径」，分类根为空串 ""
  let folderPath = initial?.folderPath ?? "";   // 文件夹视图当前路径
  let treeSelected = initial?.folderPath ?? ""; // 树视图选中节点

  const refresh = async () => {
    items = await api.listMedia("video");
    render();
  };

  const onContext = (it: MediaItem, x: number, y: number) => {
    showContextMenu(x, y, [
      { label: "编辑", onClick: () => openEditDrawer(it, refresh, activeCat) },
      {
        label: "移动",
        onClick: () => {
          // 收集当前分类下所有文件夹节点（含分类根）作为移动目标
          const tree = buildVideoTree(activeCat, items.filter(i => i.category === activeCat));
          const folders = collectFolderPaths(tree);
          const menuItems = folders.map(fp => ({
            // 显示名：分类根（空串）显示分类名，其余显示相对分类的路径
            label: fp === "" ? `${activeCat}（根）` : fp,
            disabled: fp === it.category_path,
            onClick: async () => {
              if (fp === it.category_path) return;
              try {
                await api.mediaUpdate(it.id, it.category, fp, it.title,
                  it.subtitle_path, it.cover_path, it.description);
                refresh();
              } catch (e) {
                alert("移动失败：" + e);
              }
            },
          }));
          showContextMenu(x, y, menuItems);
        },
      },
      {
        label: "删除",
        danger: true,
        onClick: async () => {
          if (confirm(`删除「${it.title}」？`)) {
            await api.mediaDelete(it.id);
            refresh();
          }
        },
      },
    ]);
  };

  const render = () => {
    const catItems = items.filter(i => i.category === activeCat);
    const tree = buildVideoTree(activeCat, catItems);
    el.innerHTML = `
      <div class="video-bar">
        <div class="tabs">${cats.map(c =>
          `<span class="tab ${c === activeCat ? "active" : ""}" data-c="${c}">${c}</span>`).join("")}</div>
        <div class="video-bar-right">
          <button class="add-video-btn">+ 新增视频</button>
          <div class="view-toggle">
            <button class="vt-btn ${mode === "folder" ? "active" : ""}" data-mode="folder" title="文件夹视图">${icon("folder", 16)}</button>
            <button class="vt-btn ${mode === "tree" ? "active" : ""}" data-mode="tree" title="树形视图">${icon("tree", 16)}</button>
          </div>
        </div>
      </div>
      <div class="video-body"></div>`;

    el.querySelectorAll<HTMLElement>(".tab").forEach(t =>
      t.onclick = () => { activeCat = t.dataset.c!; folderPath = ""; treeSelected = ""; render(); });
    el.querySelectorAll<HTMLButtonElement>(".vt-btn").forEach(b =>
      b.onclick = () => { mode = b.dataset.mode as ViewMode; render(); });
    el.querySelector<HTMLButtonElement>(".add-video-btn")!.onclick = () => openEditDrawer(null, refresh, activeCat);

    const body = el.querySelector<HTMLElement>(".video-body")!;
    body.appendChild(mode === "folder"
      ? FolderView(tree, onOpen, onContext, folderPath, (p) => { folderPath = p; })
      : TreeView(tree, onOpen, onContext, treeSelected, (p) => { treeSelected = p; }));
  };
  render();
  return el;
}
