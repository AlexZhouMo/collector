import { api } from "../lib/ipc";
import type { MediaItem } from "../lib/ipc";

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
  const gamePlayable = games.filter((g) => g.playable !== false).length;
  const gameDisabled = games.length - gamePlayable;

  const stat = (label: string, n: number, sub: string) => `
    <div class="home-stat">
      <div class="home-stat-label"><span class="home-stat-dot"></span>${label}</div>
      <div class="home-stat-num">${n}</div>
      <div class="home-stat-sub">${sub}</div>
    </div>`;

  el.innerHTML = `
    <div class="home-bg">
      <div class="home-pattern"></div>
      <div class="home-rings"></div>
      <div class="home-aurora"></div>
    </div>
    <div class="home-inner">
      <div class="home-stats">
        ${stat("影视", videos.length, `电影 ${movie} · 动漫 ${anime} · 剧集 ${tv}`)}
        ${stat("漫画", comicN, `作品数 ${comicN}`)}
        ${stat("游戏", games.length, `可用 ${gamePlayable} · 不可用 ${gameDisabled}`)}
      </div>
      <div class="home-ver">Collector&nbsp;·&nbsp;版本 <b>v${__APP_VERSION__}</b></div>
    </div>`;
  return el;
}
