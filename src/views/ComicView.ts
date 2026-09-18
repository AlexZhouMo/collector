import { api } from "../lib/ipc";
import type { MediaItem } from "../lib/ipc";
import { MediaLibraryView } from "./MediaLibraryView";

// 漫画视图：无分类；树根固定"漫画"；右键无移动项；FolderView 关闭内联重命名（enableRename=false）。
export function ComicView(onOpen: (it: MediaItem) => void): Promise<HTMLElement> {
  return MediaLibraryView({
    kind: "comic",
    extraClass: " comic-tree", // 复用 video-view 布局；comic-tree 供样式覆盖(文件夹卡与漫画卡等高)
    treeRoot: "漫画",
    supportsMove: true,
    onDelete: (id) => api.comicDelete(id),
    addLabel: "+ 新增漫画",
    enableRename: false,
  })(onOpen);
}
