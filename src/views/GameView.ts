import { api } from "../lib/ipc";
import type { MediaItem } from "../lib/ipc";
import { MediaLibraryView } from "./MediaLibraryView";

// 游戏视图：无分类；树根固定"游戏"；支持移动；FolderView 开启重命名且改名后 refresh。
export function GameView(onOpen: (it: MediaItem) => void): Promise<HTMLElement> {
  return MediaLibraryView({
    kind: "game",
    // 复用 comic-tree 的一级方形/二级长方形与卡片等高样式；额外加 game-tree 以备将来定制。
    extraClass: " game-tree comic-tree",
    treeRoot: "游戏",
    supportsMove: true,
    onDelete: (id) => api.gameDelete(id),
    addLabel: "+ 新增游戏",
    enableRename: true,
    folderRenamedRefresh: true,
  })(onOpen);
}
