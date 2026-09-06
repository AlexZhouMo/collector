import { api } from "../lib/ipc";
import type { MediaItem } from "../lib/ipc";

export async function PlayerView(it: MediaItem, onExit: () => void): Promise<HTMLElement> {
  const el = document.createElement("div");
  el.className = "player-view view-enter";
  await api.openPlayerWindow();
  await api.playerLoad(it.path, it.subtitle_path);
  let paused = false;

  el.innerHTML = `
    <div class="player-bar glass">
      <button class="back">←</button>
      <button class="rw">⏪ 10s</button>
      <button class="pp">⏸</button>
      <button class="ff">10s ⏩</button>
      <input class="seek" type="range" min="0" max="1000" value="0"/>
      <span class="time">0:00 / 0:00</span>
      <input class="vol" type="range" min="0" max="100" value="100"/>
      <button class="fs">⛶</button>
    </div>`;
  const fmt = (s: number) => `${Math.floor(s / 60)}:${String(Math.floor(s % 60)).padStart(2, "0")}`;
  const seek = el.querySelector<HTMLInputElement>(".seek")!;
  const time = el.querySelector<HTMLElement>(".time")!;

  el.querySelector<HTMLButtonElement>(".back")!.onclick = () => { cleanup(); onExit(); };
  el.querySelector<HTMLButtonElement>(".pp")!.onclick = async () => {
    paused = !paused;
    await api.playerPause(paused);
    el.querySelector(".pp")!.textContent = paused ? "▶" : "⏸";
  };
  el.querySelector<HTMLButtonElement>(".rw")!.onclick = () => api.playerSeek(-10);
  el.querySelector<HTMLButtonElement>(".ff")!.onclick = () => api.playerSeek(10);
  el.querySelector<HTMLInputElement>(".vol")!.oninput = (e) =>
    api.playerVolume(Number((e.target as HTMLInputElement).value));
  seek.onchange = async () => {
    const [, dur] = await api.playerProgress();
    api.playerSeekTo((Number(seek.value) / 1000) * dur);
  };
  el.querySelector<HTMLButtonElement>(".fs")!.onclick = () => document.documentElement.requestFullscreen?.();

  const timer = setInterval(async () => {
    try {
      const [pos, dur] = await api.playerProgress();
      if (dur > 0) {
        seek.value = String((pos / dur) * 1000);
        time.textContent = `${fmt(pos)} / ${fmt(dur)}`;
      }
    } catch {}
  }, 1000);
  const cleanup = () => clearInterval(timer);
  el.addEventListener("player-detach", cleanup);
  return el;
}
