import { api } from "../lib/ipc";
import type { MediaItem } from "../lib/ipc";
import { buildVideoTree, buildMergedVideoTree } from "../lib/videoTree";
import { FolderView } from "../components/FolderView";
import { TreeView } from "../components/TreeView";
import { showContextMenu } from "../components/ContextMenu";
import { openEditDrawer } from "../components/EditDrawer";
import { openMoveDialog } from "../components/MoveDialog";
import { icon } from "../lib/icons";

type ViewMode = "folder" | "tree";

/**
 * MediaLibraryView：Video/Comic/Game 三视图的公共实现。
 * 三者共享「顶部条 + 文件夹/树切换 + 新增 + 右键菜单」结构，差异全部由 cfg 表达。
 */
export interface MediaLibraryConfig {
  /** listMedia / editDrawer / move / FolderView.renameFolder 的媒体类型 */
  kind: "video" | "comic" | "game";
  /** 拼到 el.className 尾部（含前导空格），如 " comic-tree"。默认空串 */
  extraClass?: string;
  /**
   * 树根名：固定名（comic="漫画" / game="游戏"）；
   * 省略表示按当前分类（video，随 activeCat 变化）。
   */
  treeRoot?: string;
  /** 分类 tabs（仅 video 有）。存在则渲染 tabs 并支持切换（重置 folderPath/treeSelected） */
  cats?: string[];
  /** 右键菜单是否含「移动」项 */
  supportsMove: boolean;
  /** 删除单条记录的 API（video=mediaDelete / comic=comicDelete / game=gameDelete） */
  onDelete: (id: number) => Promise<void>;
  /** 新增按钮文案 */
  addLabel: string;
  /**
   * FolderView 的 enableRename 尾参（video/game=true，comic=false）。默认 true。
   * video 使用内联的 rename 重映射回调；comic/game 由 folderRenamedRefresh 决定是否 refresh。
   */
  enableRename?: boolean;
  /**
   * comic/game 的 onRenamed 行为：true 表示重命名后 refresh（game），false/undefined 表示不做（comic）。
   * video 不使用此项，而是走 renameRemap（重映射 folderPath 再 refresh）。
   */
  folderRenamedRefresh?: boolean;
}

export function MediaLibraryView(cfg: MediaLibraryConfig) {
  return async function (
    onOpen: (it: MediaItem) => void,
    initial?: { category: string; folderPath: string }
  ): Promise<HTMLElement> {
    const el = document.createElement("div");
    el.className = "view-enter video-view" + (cfg.extraClass ?? "");

    let items = await api.listMedia(cfg.kind);

    const cats = cfg.cats;
    // video：initial 提供且分类合法时定位该分类，否则第一个分类；非 video 无分类
    let activeCat = cats
      ? (initial && cats.includes(initial.category) ? initial.category : cats[0])
      : "";
    let mode: ViewMode = "folder";
    // 当前浏览位置（提升为视图状态，refresh 重建时保留）。
    // initial 提供时定位到该目录，否则用根（空串）。
    let folderPath = initial?.folderPath ?? "";
    let treeSelected = initial?.folderPath ?? "";

    // 树根名：固定 treeRoot（comic/game），否则用当前分类（video）
    const rootName = () => cfg.treeRoot ?? activeCat;
    // video：仅取当前分类的 items；固定根：全部 items
    const treeItems = () => (cats ? items.filter(i => i.category === activeCat) : items);
    const editCategory = () => (cats ? activeCat : "");

    const refresh = async () => {
      items = await api.listMedia(cfg.kind);
      render();
    };

    const onContext = (it: MediaItem, x: number, y: number) => {
      const menu: Array<{ label: string; danger?: boolean; onClick: () => void }> = [
        { label: "编辑", onClick: () => openEditDrawer(it, refresh, editCategory(), cfg.kind) },
      ];
      if (cfg.supportsMove) {
        menu.push({
          label: "移动",
          onClick: () => {
            if (cats) {
              // 影视：三分类合一树 + 跨分类模式（items 是当前 kind 全部项，含电影/动漫/剧集）
              const mergedTree = buildMergedVideoTree(
                cats.map(c => [c, items.filter(i => i.category === c)])
              );
              openMoveDialog(it, mergedTree, cfg.kind, refresh, true);
            } else {
              // 漫画/游戏：单分类树，不跨分类
              const tree = buildVideoTree(rootName(), treeItems());
              openMoveDialog(it, tree, cfg.kind, refresh);
            }
          },
        });
      }
      menu.push({
        label: "删除",
        danger: true,
        onClick: async () => {
          if (confirm(`删除「${it.title}」？`)) {
            await cfg.onDelete(it.id);
            refresh();
          }
        },
      });
      showContextMenu(x, y, menu);
    };

    const render = () => {
      const tree = buildVideoTree(rootName(), treeItems());
      const tabsHtml = cats
        ? cats.map(c => `<span class="tab ${c === activeCat ? "active" : ""}" data-c="${c}">${c}</span>`).join("")
        : "";
      el.innerHTML = `
        <div class="video-bar">
          <div class="tabs">${tabsHtml}</div>
          <div class="video-bar-right">
            <button class="add-btn">${cfg.addLabel}</button>
            <div class="view-toggle">
              <button class="vt-btn ${mode === "folder" ? "active" : ""}" data-mode="folder" title="文件夹视图">${icon("folder", 16)}</button>
              <button class="vt-btn ${mode === "tree" ? "active" : ""}" data-mode="tree" title="树形视图">${icon("tree", 16)}</button>
            </div>
          </div>
        </div>
        <div class="video-body"></div>`;

      if (cats) {
        el.querySelectorAll<HTMLElement>(".tab").forEach(t =>
          t.onclick = () => { activeCat = t.dataset.c!; folderPath = ""; treeSelected = ""; render(); });
      }
      el.querySelectorAll<HTMLButtonElement>(".vt-btn").forEach(b =>
        b.onclick = () => { mode = b.dataset.mode as ViewMode; render(); });
      const addBtn = el.querySelector<HTMLButtonElement>(".add-btn")!;
      addBtn.onclick = () => openEditDrawer(null, refresh, editCategory(), cfg.kind, folderPath);
      const updateAddBtn = () => {
        addBtn.style.display = (mode === "folder" && folderPath !== "") ? "" : "none";
      };
      updateAddBtn();

      const body = el.querySelector<HTMLElement>(".video-body")!;
      if (mode === "folder") {
        const onNav = (p: string) => { folderPath = p; updateAddBtn(); };
        // onRenamed：video 重映射 folderPath 并 refresh；game refresh；comic 不做（undefined）
        const onRenamed = cats
          ? (oldPath: string, newPath: string) => {
              if (folderPath === oldPath) folderPath = newPath;
              else if (folderPath.startsWith(oldPath + "/")) folderPath = newPath + folderPath.slice(oldPath.length);
              refresh();
            }
          : cfg.folderRenamedRefresh
            ? () => { refresh(); }
            : undefined;
        body.appendChild(
          FolderView(tree, onOpen, onContext, folderPath, onNav, onRenamed, cfg.enableRename ?? true, cfg.kind)
        );
      } else {
        body.appendChild(TreeView(tree, onOpen, onContext, treeSelected, (p) => { treeSelected = p; }));
      }
    };
    render();
    return el;
  };
}
