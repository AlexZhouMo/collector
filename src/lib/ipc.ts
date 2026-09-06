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
};
