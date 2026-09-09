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
  // video 加载/解码错误 → 显示具体原因（诊断 + 用户提示）
  const MEDIA_ERR: Record<number, string> = {
    1: "加载被中止",
    2: "网络错误",
    3: "解码失败（编码不受支持）",
    4: "源不可用或格式不支持",
  };
  video.addEventListener("error", () => {
    const code = video.error?.code ?? 0;
    const msg = MEDIA_ERR[code] || `未知错误(${code})`;
    loading.textContent = "视频加载失败：" + msg;
    loading.style.display = "";
    video.style.display = "none";
    console.error("[player] video error", code, video.error?.message, "src=", video.src);
  });
  // 能播放了才隐藏 loading、显示 video
  video.addEventListener("canplay", () => {
    if (closed) return;
    loading.style.display = "none";
    video.style.display = "";
    video.play().catch(() => {});
  }, { once: true });

  try {
    loading.textContent = "准备中…（转封装视频）";
    const info = await api.playerOpen(it.category, it.category_path, it.title);
    if (closed) return el;
    duration = info.duration;
    // info.src 已是本地 HTTP server 的 URL（http://127.0.0.1:port/xxx.mp4，支持 Range）
    console.log("[player] opened", { src: info.src, duration });
    loading.textContent = "加载中…";
    video.src = info.src;
    video.load();
  } catch (e) {
    loading.textContent = "无法播放该视频：" + e;
    console.error("[player] playerOpen failed", e);
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
  // CSS 伪全屏：让 .player-view 铺满整个应用窗口。不用 video.requestFullscreen()
  // ——WKWebView 对元素级 Fullscreen API 支持不稳定，CSS fixed 铺满 100% 可靠。
  const toggleFullscreen = () => el.classList.toggle("fullscreen");
  el.querySelector<HTMLButtonElement>(".fs")!.onclick = toggleFullscreen;
  const onKey = (e: KeyboardEvent) => {
    if (e.key === "Escape" && el.classList.contains("fullscreen")) el.classList.remove("fullscreen");
  };
  document.addEventListener("keydown", onKey);

  video.ontimeupdate = () => {
    if (seeking || closed) return;
    const d = dur();
    if (d > 0) {
      seek.value = String((video.currentTime / d) * 1000);
      time.textContent = `${fmt(video.currentTime)} / ${fmt(d)}`;
    }
  };

  const cleanup = () => {
    if (closed) return;
    closed = true;
    document.removeEventListener("keydown", onKey);
    video.pause();
    video.src = "";
    api.playerStop().catch(() => {});
  };
  el.querySelector<HTMLButtonElement>(".back")!.onclick = () => { cleanup(); onExit(); };
  el.addEventListener("player-detach", cleanup);
  return el;
}
