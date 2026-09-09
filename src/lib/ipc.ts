import { invoke } from "@tauri-apps/api/core";

export interface MediaItem {
  id: number;
  category: string;
  category_path: string;
  title: string;
  subtitle_path: string | null;
  cover_path: string | null;
  description: string | null;
  playable: boolean;
  video_path: string;
}

export interface SubIssue { line: number; kind: string; text: string; }
export interface SubReport { file: string; issues: SubIssue[]; }

export interface FailedItem { category: string; category_path: string; title: string; reason: string; suggest_name: string | null; suggest_note: string; }
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
  playerOpen: (category: string, categoryPath: string, title: string) =>
    invoke<{ src: string; duration: number }>("player_open", { category, categoryPath, title }),
  playerStop: () => invoke<void>("player_stop"),
  launchGame: (categoryPath: string, title: string) =>
    invoke<void>("launch_game", { categoryPath, title }),
  normalizeSubtitles: (inDir: string, outDir: string) =>
    invoke<SubReport[]>("normalize_subtitles", { inDir, outDir }),
  normalizeComic: (dir: string, prefix: string, outZip: string) =>
    invoke<number>("normalize_comic", { dir, prefix, outZip }),
  mediaUpdate: (id: number, category: string, categoryPath: string, title: string,
    subtitlePath: string | null, coverPath: string | null, description: string | null) =>
    invoke<void>("media_update", { id, category, categoryPath, title, subtitlePath, coverPath, description }),
  mediaCreate: (category: string, categoryPath: string, title: string,
    subtitlePath: string | null, coverPath: string | null, description: string | null) =>
    invoke<number>("media_create", { category, categoryPath, title, subtitlePath, coverPath, description }),
  mediaDelete: (id: number) => invoke<void>("media_delete", { id }),
  importCover: (srcImage: string) => invoke<string>("import_cover", { srcImage }),
  importCoverCropped: (srcImage: string, x: number, y: number, w: number, h: number) =>
    invoke<string>("import_cover_cropped", { srcImage, x, y, w, h }),
  deleteCoverFile: (path: string) => invoke<void>("delete_cover_file", { path }),
  setTmdbKey: (key: string) => invoke<void>("set_tmdb_key", { key }),
  getTmdbKey: () => invoke<string | null>("get_tmdb_key"),
  fetchPosters: () => invoke<FetchReport>("fetch_posters"),
};
