import { api } from "../lib/ipc";
import type { MediaItem } from "../lib/ipc";
import { icon } from "../lib/icons";
import { esc } from "../lib/escape";

export async function PlayerView(it: MediaItem, onExit: () => void): Promise<HTMLElement> {
  const el = document.createElement("div");
  el.className = "player-view view-enter";
  el.innerHTML = `
    <div class="player-top">
      <button class="back icon-text">${icon("arrowLeft", 16)}<span>返回</span></button>
      <span class="player-title">${esc(it.title)}</span>
    </div>
    <div class="player-stage-wrap">
      <video class="player-video" playsinline></video>
    </div>
    <div class="player-bar glass">
      <button class="pp">${icon("pause", 18)}</button>
      <button class="rw">${icon("rewind", 18)}</button>
      <button class="ff">${icon("forward", 18)}</button>
      <input class="seek" type="range" min="0" max="1000" value="0"/>
      <span class="time">0:00 / 0:00</span>
      <input class="vol" type="range" min="0" max="100" value="100"/>
      <button class="fs">${icon("fullscreen", 18)}</button>
    </div>`;

  const video = el.querySelector<HTMLVideoElement>(".player-video")!;
  const seek = el.querySelector<HTMLInputElement>(".seek")!;
  const time = el.querySelector<HTMLElement>(".time")!;
  const fmt = (s: number) => `${Math.floor(s / 60)}:${String(Math.floor(s % 60)).padStart(2, "0")}`;
  let duration = 0;
  let seekBase = 0;
  let seeking = false;
  let closed = false;

  duration = await api.playerOpen(it.path);
  video.src = "stream://localhost/current";
  video.load();
  video.play().catch(() => {});
  api.getVideoPos(it.id).then((resume) => {
    if (resume > 5 && !closed) doSeek(resume);
  }).catch(() => {});

  function absPos() { return seekBase + video.currentTime; }

  async function doSeek(target: number) {
    seekBase = target;
    await api.playerSeek(it.path, target);
    video.src = "stream://localhost/current?t=" + Date.now();
    video.load();
    video.play().catch(() => {});
  }

  el.querySelector<HTMLButtonElement>(".pp")!.onclick = () => {
    if (video.paused) { video.play(); el.querySelector(".pp")!.innerHTML = icon("pause", 18); }
    else { video.pause(); el.querySelector(".pp")!.innerHTML = icon("play", 18); }
  };
  el.querySelector<HTMLButtonElement>(".rw")!.onclick = () => doSeek(Math.max(0, absPos() - 10));
  el.querySelector<HTMLButtonElement>(".ff")!.onclick = () => doSeek(Math.min(duration, absPos() + 10));
  el.querySelector<HTMLInputElement>(".vol")!.oninput = (e) =>
    video.volume = Number((e.target as HTMLInputElement).value) / 100;
  seek.oninput = () => { seeking = true; };
  seek.onchange = () => {
    const target = (Number(seek.value) / 1000) * duration;
    doSeek(target);
    setTimeout(() => { seeking = false; }, 300);
  };
  el.querySelector<HTMLButtonElement>(".fs")!.onclick = () => {
    if (!document.fullscreenElement) video.requestFullscreen?.();
    else document.exitFullscreen?.();
  };

  video.ontimeupdate = () => {
    if (seeking || closed) return;
    const pos = absPos();
    if (duration > 0) {
      seek.value = String((pos / duration) * 1000);
      time.textContent = `${fmt(pos)} / ${fmt(duration)}`;
    }
    api.setVideoPos(it.id, pos).catch(() => {});
  };

  const cleanup = () => {
    if (closed) return;
    closed = true;
    if (absPos() > 0) api.setVideoPos(it.id, absPos()).catch(() => {});
    video.pause();
    video.src = "";
    api.playerStop().catch(() => {});
  };
  el.querySelector<HTMLButtonElement>(".back")!.onclick = () => { cleanup(); onExit(); };
  el.addEventListener("player-detach", cleanup);
  return el;
}
