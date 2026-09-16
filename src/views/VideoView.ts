import { api } from "../lib/ipc";
import type { MediaItem } from "../lib/ipc";
import { MediaLibraryView } from "./MediaLibraryView";

// 视频视图：分类 tabs（电影/动漫/剧集）、支持移动、支持从播放器返回的 initial 定位。
export function VideoView(
  onOpen: (it: MediaItem) => void,
  initial?: { category: string; folderPath: string }
): Promise<HTMLElement> {
  return MediaLibraryView({
    kind: "video",
    cats: ["电影", "动漫", "剧集"],
    supportsMove: true,
    onDelete: (id) => api.mediaDelete(id),
    addLabel: "+ 新增视频",
    // treeRoot 省略：树根随 activeCat；enableRename 默认 true；rename 走 video 重映射逻辑
  })(onOpen, initial);
}
