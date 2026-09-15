import { api } from "../lib/ipc";
import type { MediaItem } from "../lib/ipc";
import { icon } from "../lib/icons";

/** 主页：拉取三类素材，前端聚合细分统计，渲染科技感概览。 */
export async function HomeView(): Promise<HTMLElement> {
  const el = document.createElement("div");
  el.className = "view-enter home-view";

  // 并发拉取三类；任一失败降级为空数组，保证页面可渲染（计数显示 0）。
  const [videos, comics, games] = await Promise.all([
    api.listMedia("video").catch(() => [] as MediaItem[]),
    api.listMedia("comic").catch(() => [] as MediaItem[]),
    api.listMedia("game").catch(() => [] as MediaItem[]),
  ]);

  const countBy = (arr: MediaItem[], cat: string) =>
    arr.filter((i) => i.category === cat).length;

  const movie = countBy(videos, "电影");
  const anime = countBy(videos, "动漫");
  const tv = countBy(videos, "剧集");
  const comicN = comics.length;

  const stat = (label: string, ic: "film" | "book" | "gamepad", n: number, sub: string) => `
    <div class="home-stat">
      <div class="home-stat-label"><span class="home-stat-ic">${icon(ic, 22)}</span>${label}</div>
      <div class="home-stat-num">${n}</div>
      ${sub ? `<div class="home-stat-sub">${sub}</div>` : ""}
    </div>`;

  el.innerHTML = `
    <div class="home-bg"></div>
    <div class="home-inner">
      <div class="home-stats">
        ${stat("影视", "film", videos.length, `电影 ${movie} · 动漫 ${anime} · 剧集 ${tv}`)}
        ${stat("漫画", "book", comicN, "")}
        ${stat("游戏", "gamepad", games.length, "")}
      </div>
      <div class="home-ver">Collector&nbsp;·&nbsp;版本 <b>v${__APP_VERSION__}</b></div>
    </div>`;
  return el;
}
