import { invoke } from "@tauri-apps/api/core";

export interface MediaItem {
  id: number;
  kind: "video" | "comic" | "game";
  category: string;
  category_path: string;
  title: string;
  path: string;
  subtitle_path: string | null;
  cover_path: string | null;
  description: string | null;
  platform_ok: boolean;
  exec_path: string | null;
}

export interface SubIssue { line: number; kind: string; text: string; }
export interface SubReport { file: string; issues: SubIssue[]; }

export interface FailedItem { title: string; reason: string; }
export interface FetchReport { ok: number; failed: FailedItem[]; }

export const api = {
  setRoot: (kind: string, path: string) => invoke<void>("set_root", { kind, path }),
  getRoot: (kind: string) => invoke<string | null>("get_root", { kind }),
  scanRoot: (kind: string) => invoke<number>("scan_root", { kind }),
  scanVideos: () => invoke<number>("scan_videos_all"),
  initFromDemo: (demoRoot: string) => invoke<number>("init_from_demo", { demoRoot }),
  listMedia: (kind: string) => invoke<MediaItem[]>("list_media", { kind }),
  comicPages: (path: string) => invoke<string[]>("comic_pages", { path }),
  comicPage: (path: string, entry: string) => invoke<string>("comic_page", { path, entry }),
  comicCover: (path: string) => invoke<string | null>("comic_cover", { path }),
  setComicPage: (itemId: number, page: number) => invoke<void>("set_comic_page", { itemId, page }),
  getComicPage: (itemId: number) => invoke<number>("get_comic_page", { itemId }),
  setVideoPos: (itemId: number, secs: number) => invoke<void>("set_video_pos", { itemId, secs }),
  getVideoPos: (itemId: number) => invoke<number>("get_video_pos", { itemId }),
  playerOpen: (path: string) => invoke<{ src: string; duration: number }>("player_open", { path }),
  playerStop: () => invoke<void>("player_stop"),
  launchGame: (itemId: number, execPath: string) => invoke<void>("launch_game", { itemId, execPath }),
  normalizeSubtitles: (inDir: string, outDir: string) =>
    invoke<SubReport[]>("normalize_subtitles", { inDir, outDir }),
  normalizeComic: (dir: string, prefix: string, outZip: string) =>
    invoke<number>("normalize_comic", { dir, prefix, outZip }),
  mediaUpdate: (id: number, category: string, categoryPath: string, title: string,
    path: string, subtitlePath: string | null, coverPath: string | null, description: string | null) =>
    invoke<void>("media_update", { id, category, categoryPath, title, path, subtitlePath, coverPath, description }),
  mediaCreate: (category: string, categoryPath: string, title: string,
    path: string, subtitlePath: string | null, coverPath: string | null, description: string | null) =>
    invoke<number>("media_create", { category, categoryPath, title, path, subtitlePath, coverPath, description }),
  mediaDelete: (id: number) => invoke<void>("media_delete", { id }),
  importCover: (srcImage: string) => invoke<string>("import_cover", { srcImage }),
  setTmdbKey: (key: string) => invoke<void>("set_tmdb_key", { key }),
  getTmdbKey: () => invoke<string | null>("get_tmdb_key"),
  fetchPosters: () => invoke<FetchReport>("fetch_posters"),
};
