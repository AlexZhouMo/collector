import { api } from "../lib/ipc";
import type { MediaItem } from "../lib/ipc";
import { icon } from "../lib/icons";
import { esc } from "../lib/escape";
import { getCurrentWindow } from "@tauri-apps/api/window";

export async function PlayerView(it: MediaItem, onExit: () => void): Promise<HTMLElement> {
  const el = document.createElement("div");
  el.className = "player-view view-enter";
  el.innerHTML = `
    <div class="player-top">
      <button class="back icon-text">${icon("arrowLeft", 16)}<span>返回</span></button>
      <span class="player-title">${esc(it.title)}</span>
    </div>
    <div class="player-stage"></div>
    <div class="player-bar glass">
      <button class="rw">${icon("rewind", 18)}</button>
      <button class="pp">${icon("pause", 18)}</button>
      <button class="ff">${icon("forward", 18)}</button>
      <input class="seek" type="range" min="0" max="1000" value="0"/>
      <span class="time">0:00 / 0:00</span>
      <input class="vol" type="range" min="0" max="100" value="100"/>
      <button class="fs">${icon("fullscreen", 18)}</button>
    </div>`;

  const stage = el.querySelector<HTMLElement>(".player-stage")!;
  const seek = el.querySelector<HTMLInputElement>(".seek")!;
  const time = el.querySelector<HTMLElement>(".time")!;
  const fmt = (s: number) => `${Math.floor(s / 60)}:${String(Math.floor(s % 60)).padStart(2, "0")}`;
  let paused = false, fullscreen = false, seeking = false, lastPos = 0, closed = false;

  const appWin = getCurrentWindow();
  // 计算 stage 在屏幕上的逻辑坐标：主窗内容区左上(inner) 换算逻辑像素 + stage 相对视口 rect
  async function stageBounds() {
    const r = stage.getBoundingClientRect();
    const inner = await appWin.innerPosition(); // 物理像素
    const sf = await appWin.scaleFactor();
    const ix = inner.x / sf, iy = inner.y / sf;
    return { x: ix + r.left, y: iy + r.top, width: r.width, height: r.height };
  }
  async function positionMpv() {
    const b = await stageBounds();
    await api.playerSetBounds(b.x, b.y, b.width, b.height);
  }

  // 初始化：必须等 el 挂载进 DOM 且完成布局后再测量 stage，否则
  // getBoundingClientRect() 尺寸为 0，mpv 窗口会以错误的小尺寸创建（视频不充满）。
  // 用双 requestAnimationFrame 确保挂载 + 布局完成后再定位 mpv。
  requestAnimationFrame(() => requestAnimationFrame(async () => {
    if (closed) return;
    const b0 = await stageBounds();
    await api.openPlayerWindow(b0.x, b0.y, b0.width, b0.height);
    await api.playerLoad(it.path, it.subtitle_path);
    api.getVideoPos(it.id).then((resume) => {
      if (resume > 5) setTimeout(() => { if (!closed) api.playerSeekTo(resume).catch(() => {}); }, 300);
    }).catch(() => {});
  }));

  const ro = new ResizeObserver(() => { if (!closed) positionMpv().catch(() => {}); });
  ro.observe(stage);
  const unlistenMovedP = appWin.onMoved(() => { if (!closed) positionMpv().catch(() => {}); });

  el.querySelector<HTMLButtonElement>(".pp")!.onclick = async () => {
    paused = !paused;
    await api.playerPause(paused);
    el.querySelector(".pp")!.innerHTML = icon(paused ? "play" : "pause", 18);
  };
  el.querySelector<HTMLButtonElement>(".rw")!.onclick = () => api.playerSeek(-10);
  el.querySelector<HTMLButtonElement>(".ff")!.onclick = () => api.playerSeek(10);
  el.querySelector<HTMLInputElement>(".vol")!.oninput = (e) =>
    api.playerVolume(Number((e.target as HTMLInputElement).value));
  seek.oninput = () => { seeking = true; };
  seek.onchange = async () => {
    const [, dur] = await api.playerProgress();
    api.playerSeekTo((Number(seek.value) / 1000) * dur);
    setTimeout(() => { seeking = false; }, 600);
  };
  el.querySelector<HTMLButtonElement>(".fs")!.onclick = async () => {
    fullscreen = !fullscreen;
    await api.playerFullscreen(fullscreen);
  };

  const timer = setInterval(async () => {
    if (seeking || closed) return;
    try {
      const [pos, dur] = await api.playerProgress();
      if (dur > 0) {
        seek.value = String((pos / dur) * 1000);
        time.textContent = `${fmt(pos)} / ${fmt(dur)}`;
      }
      lastPos = pos;
      api.setVideoPos(it.id, pos).catch(() => {});
    } catch {}
  }, 1000);

  // 三步退出：停播卸载 → 藏窗口 → 异步 drop 实例（不阻塞前台）
  const cleanup = async () => {
    if (closed) return;
    closed = true;
    clearInterval(timer);
    ro.disconnect();
    unlistenMovedP.then((un) => un()).catch(() => {});
    if (lastPos > 0) api.setVideoPos(it.id, lastPos).catch(() => {});
    await api.playerStop().catch(() => {});
    await api.playerCloseWindow().catch(() => {});
    api.playerClose().catch(() => {}); // 异步 drop，不 await
  };
  el.querySelector<HTMLButtonElement>(".back")!.onclick = async () => {
    await cleanup();
    onExit();
  };
  el.addEventListener("player-detach", () => { cleanup(); });
  return el;
}
