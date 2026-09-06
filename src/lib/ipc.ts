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

export const api = {
  setRoot: (kind: string, path: string) => invoke<void>("set_root", { kind, path }),
  getRoot: (kind: string) => invoke<string | null>("get_root", { kind }),
  scanRoot: (kind: string) => invoke<number>("scan_root", { kind }),
  listMedia: (kind: string) => invoke<MediaItem[]>("list_media", { kind }),
  comicPages: (path: string) => invoke<string[]>("comic_pages", { path }),
  comicPage: (path: string, entry: string) => invoke<string>("comic_page", { path, entry }),
  comicCover: (path: string) => invoke<string | null>("comic_cover", { path }),
  setComicPage: (itemId: number, page: number) => invoke<void>("set_comic_page", { itemId, page }),
  getComicPage: (itemId: number) => invoke<number>("get_comic_page", { itemId }),
  setVideoPos: (itemId: number, secs: number) => invoke<void>("set_video_pos", { itemId, secs }),
  getVideoPos: (itemId: number) => invoke<number>("get_video_pos", { itemId }),
  openPlayerWindow: () => invoke<void>("open_player_window"),
  playerLoad: (path: string, subtitle: string | null) => invoke<void>("player_load", { path, subtitle }),
  playerPause: (paused: boolean) => invoke<void>("player_pause", { paused }),
  playerSeek: (secs: number) => invoke<void>("player_seek", { secs }),
  playerSeekTo: (secs: number) => invoke<void>("player_seek_to", { secs }),
  playerVolume: (vol: number) => invoke<void>("player_volume", { vol }),
  playerProgress: () => invoke<[number, number]>("player_progress"),
  playerFullscreen: (on: boolean) => invoke<void>("player_fullscreen", { on }),
  launchGame: (itemId: number, execPath: string) => invoke<void>("launch_game", { itemId, execPath }),
};
