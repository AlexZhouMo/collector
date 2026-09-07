import { api } from "../lib/ipc";
import type { MediaItem } from "../lib/ipc";
import { icon } from "../lib/icons";

export async function PlayerView(it: MediaItem, onExit: () => void): Promise<HTMLElement> {
  const el = document.createElement("div");
  el.className = "player-view view-enter";
  await api.openPlayerWindow();
  await api.playerLoad(it.path, it.subtitle_path);
  // 续播：若已保存进度(>5s)则跳过去。loadfile 是异步的，
  // 刚 load 完立刻 seek 可能因文件尚未就绪而无效，故延迟 300ms 再 seek。
  api.getVideoPos(it.id).then((resume) => {
    if (resume > 5) setTimeout(() => api.playerSeekTo(resume), 300);
  }).catch(() => {});
  let paused = false;
  let fullscreen = false;
  let seeking = false;
  let lastPos = 0;

  el.innerHTML = `
    <div class="player-bar glass">
      <button class="back">${icon("arrowLeft", 18)}</button>
      <button class="rw">${icon("rewind", 18)}</button>
      <button class="pp">${icon("pause", 18)}</button>
      <button class="ff">${icon("forward", 18)}</button>
      <input class="seek" type="range" min="0" max="1000" value="0"/>
      <span class="time">0:00 / 0:00</span>
      <input class="vol" type="range" min="0" max="100" value="100"/>
      <button class="fs">${icon("fullscreen", 18)}</button>
    </div>`;
  const fmt = (s: number) => `${Math.floor(s / 60)}:${String(Math.floor(s % 60)).padStart(2, "0")}`;
  const seek = el.querySelector<HTMLInputElement>(".seek")!;
  const time = el.querySelector<HTMLElement>(".time")!;

  el.querySelector<HTMLButtonElement>(".back")!.onclick = () => { cleanup(); onExit(); };
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
    if (seeking) return;
    try {
      const [pos, dur] = await api.playerProgress();
      if (dur > 0) {
        seek.value = String((pos / dur) * 1000);
        time.textContent = `${fmt(pos)} / ${fmt(dur)}`;
      }
      // 定时保存当前进度用于续播（seeking 时已 return，不保存无妨）
      lastPos = pos;
      api.setVideoPos(it.id, pos).catch(() => {});
    } catch {}
  }, 1000);
  const cleanup = () => {
    clearInterval(timer);
    // 退出前再保存一次最终进度
    if (lastPos > 0) api.setVideoPos(it.id, lastPos).catch(() => {});
  };
  el.addEventListener("player-detach", cleanup);
  return el;
}
