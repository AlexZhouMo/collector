import { api } from "../lib/ipc";
import type { MediaItem } from "../lib/ipc";
import { icon } from "../lib/icons";
import { esc } from "../lib/escape";
import { convertFileSrc } from "@tauri-apps/api/core";

export async function PlayerView(it: MediaItem, onExit: () => void): Promise<HTMLElement> {
  const el = document.createElement("div");
  el.className = "player-view view-enter";
  el.innerHTML = `
    <div class="player-top">
      <button class="back icon-text">${icon("arrowLeft", 16)}<span>返回</span></button>
      <span class="player-title">${esc(it.title)}</span>
    </div>
    <div class="player-stage-wrap">
      <div class="player-loading">准备中…</div>
      <video class="player-video" playsinline style="display:none"></video>
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
  const loading = el.querySelector<HTMLElement>(".player-loading")!;
  const seek = el.querySelector<HTMLInputElement>(".seek")!;
  const time = el.querySelector<HTMLElement>(".time")!;
  const fmt = (s: number) => `${Math.floor(s / 60)}:${String(Math.floor(s % 60)).padStart(2, "0")}`;
  let duration = 0;
  let seeking = false;
  let closed = false;
  const dur = () => duration || video.duration || 0;

  // remux 成临时 mp4（秒级），再用 asset:// 播放本地文件（原生 seek/进度）
  try {
    const info = await api.playerOpen(it.path);
    if (closed) return el;
    duration = info.duration;
    video.src = convertFileSrc(info.src);
    video.style.display = "";
    loading.style.display = "none";
    video.play().catch(() => {});
    const resume = await api.getVideoPos(it.id).catch(() => 0);
    if (resume > 5 && !closed) video.currentTime = resume;
  } catch (e) {
    loading.textContent = "无法播放该视频：" + e;
  }

  el.querySelector<HTMLButtonElement>(".pp")!.onclick = () => {
    if (video.paused) { video.play(); el.querySelector(".pp")!.innerHTML = icon("pause", 18); }
    else { video.pause(); el.querySelector(".pp")!.innerHTML = icon("play", 18); }
  };
  el.querySelector<HTMLButtonElement>(".rw")!.onclick = () => { video.currentTime = Math.max(0, video.currentTime - 10); };
  el.querySelector<HTMLButtonElement>(".ff")!.onclick = () => { video.currentTime = Math.min(dur(), video.currentTime + 10); };
  el.querySelector<HTMLInputElement>(".vol")!.oninput = (e) =>
    video.volume = Number((e.target as HTMLInputElement).value) / 100;
  seek.oninput = () => { seeking = true; };
  seek.onchange = () => {
    video.currentTime = (Number(seek.value) / 1000) * dur();
    seeking = false;
  };
  el.querySelector<HTMLButtonElement>(".fs")!.onclick = () => {
    if (!document.fullscreenElement) video.requestFullscreen?.();
    else document.exitFullscreen?.();
  };

  video.ontimeupdate = () => {
    if (seeking || closed) return;
    const d = dur();
    if (d > 0) {
      seek.value = String((video.currentTime / d) * 1000);
      time.textContent = `${fmt(video.currentTime)} / ${fmt(d)}`;
    }
    api.setVideoPos(it.id, video.currentTime).catch(() => {});
  };

  const cleanup = () => {
    if (closed) return;
    closed = true;
    if (video.currentTime > 0) api.setVideoPos(it.id, video.currentTime).catch(() => {});
    video.pause();
    video.src = "";
    api.playerStop().catch(() => {});
  };
  el.querySelector<HTMLButtonElement>(".back")!.onclick = () => { cleanup(); onExit(); };
  el.addEventListener("player-detach", cleanup);
  return el;
}
