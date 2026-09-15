import { api } from "../lib/ipc";
import type { MediaItem } from "../lib/ipc";
import { icon } from "../lib/icons";
import { esc } from "../lib/escape";
import { convertFileSrc } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { SubtitleRenderer } from "../components/SubtitleRenderer";

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
      <div class="player-video-box">
        <video class="player-video" playsinline style="display:none"></video>
      </div>
    </div>
    <div class="player-bar glass">
      <button class="pp">${icon("pause", 18)}</button>
      <button class="rw">${icon("rewind", 18)}</button>
      <button class="ff">${icon("forward", 18)}</button>
      <input class="seek" type="range" min="0" max="1000" value="0"/>
      <span class="time">0:00 / 0:00</span>
      <input class="vol" type="range" min="0" max="100" value="100"/>
      <button class="cc" title="字幕" style="display:none">${icon("captions", 18)}</button>
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
  let sub: SubtitleRenderer | null = null;
  let subtitleOn = true;
  let pendingSubUrl: string | null = null;
  const ccBtn = el.querySelector<HTMLButtonElement>(".cc")!;
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
    // 字幕等 video 已挂载 DOM 且可见（有尺寸）后再初始化：
    // libass 渲染器按 video 的显示尺寸定位字幕 canvas，video 游离/隐藏时尺寸为 0 会定位错误。
    if (pendingSubUrl && !sub) {
      sub = new SubtitleRenderer(video, pendingSubUrl);
      ccBtn.style.display = "";
      ccBtn.classList.add("cc-on");
    }
  }, { once: true });

  try {
    loading.textContent = "准备中…（转封装视频）";
    const info = await api.playerOpen(it.category, it.category_path, it.title);
    if (closed) return el;
    duration = info.duration;
    // info.src 已是本地 HTTP server 的 URL（http://127.0.0.1:port/xxx.mp4，支持 Range）
    loading.textContent = "加载中…";
    video.src = info.src;
    video.load();
    // 记下字幕 URL，待 canplay（video 有尺寸）后再初始化字幕渲染器
    if (info.subtitle) pendingSubUrl = convertFileSrc(info.subtitle);
  } catch (e) {
    loading.style.whiteSpace = "pre-line";
    loading.style.textAlign = "left";
    loading.style.maxWidth = "560px";
    loading.style.lineHeight = "1.6";
    loading.textContent = "无法播放该视频：\n\n" + e;
    console.error("[player] playerOpen failed", e);
  }

  const togglePlay = () => {
    if (video.paused) { video.play(); el.querySelector(".pp")!.innerHTML = icon("pause", 18); }
    else { video.pause(); el.querySelector(".pp")!.innerHTML = icon("play", 18); }
  };
  const rewind = () => { video.currentTime = Math.max(0, video.currentTime - 10); };
  const forward = () => { video.currentTime = Math.min(dur(), video.currentTime + 10); };
  el.querySelector<HTMLButtonElement>(".pp")!.onclick = togglePlay;
  el.querySelector<HTMLButtonElement>(".rw")!.onclick = rewind;
  el.querySelector<HTMLButtonElement>(".ff")!.onclick = forward;
  el.querySelector<HTMLInputElement>(".vol")!.oninput = (e) =>
    video.volume = Number((e.target as HTMLInputElement).value) / 100;
  seek.oninput = () => { seeking = true; };
  seek.onchange = () => {
    video.currentTime = (Number(seek.value) / 1000) * dur();
    seeking = false;
  };
  // 全屏：Tauri 原生窗口全屏（setFullscreen，铺满物理屏幕）叠加 CSS .fullscreen 铺满窗口。
  // 不用元素级 requestFullscreen——WKWebView 对其支持不稳定。
  // 全屏时控制面板（顶栏+播放条）静止 1.5s 后自动隐藏、隐藏鼠标；鼠标移动即恢复。
  let hideTimer: number | undefined;
  const showControls = () => {
    el.classList.remove("controls-hidden");
    clearTimeout(hideTimer);
    if (el.classList.contains("fullscreen")) {
      hideTimer = window.setTimeout(() => el.classList.add("controls-hidden"), 1500);
    }
  };
  const onMouseMove = () => showControls();
  const setNativeFullscreen = async (on: boolean) => {
    try { await getCurrentWindow().setFullscreen(on); }
    catch (e) { console.error("[player] setFullscreen failed", e); }
  };
  const enterFullscreen = async () => {
    await setNativeFullscreen(true);
    el.classList.add("fullscreen");
    showControls();
  };
  const exitFullscreen = async () => {
    await setNativeFullscreen(false);
    el.classList.remove("fullscreen");
    showControls();
  };
  const toggleFullscreen = () => {
    if (el.classList.contains("fullscreen")) exitFullscreen();
    else enterFullscreen();
  };
  el.querySelector<HTMLButtonElement>(".fs")!.onclick = toggleFullscreen;
  // 监听原生全屏状态变化：用户可能用系统按钮/手势退出全屏（非经我们的按钮/Esc），
  // 需回读真实状态同步 CSS 类，避免 UI 与窗口状态失步。
  let unlistenResize: (() => void) | null = null;
  const win = getCurrentWindow();
  win.onResized(async () => {
    if (closed) return;
    let native = false;
    try { native = await win.isFullscreen(); } catch { return; }
    const cssFull = el.classList.contains("fullscreen");
    if (native === cssFull) return;
    if (native) { el.classList.add("fullscreen"); }
    else { el.classList.remove("fullscreen"); }
    showControls();
  }).then((un) => {
    if (closed) { un(); return; }  // 若已在注册完成前 cleanup，立即注销
    unlistenResize = un;
  });
  el.addEventListener("mousemove", onMouseMove);
  ccBtn.onclick = () => {
    subtitleOn = !subtitleOn;
    sub?.setVisible(subtitleOn);
    ccBtn.classList.toggle("cc-on", subtitleOn);
  };
  // 键盘快捷键：空格播放/暂停，左右方向键快退/快进，Esc 退出全屏。
  // 焦点落在滑块(range)上时不接管方向键——让其原生调节音量/进度。
  const onKey = (e: KeyboardEvent) => {
    if (closed) return;
    const onSlider = e.target instanceof HTMLInputElement && e.target.type === "range";
    if (e.key === " " || e.code === "Space") {
      e.preventDefault();
      togglePlay();
    } else if (e.key === "ArrowLeft" && !onSlider) {
      e.preventDefault();
      rewind();
    } else if (e.key === "ArrowRight" && !onSlider) {
      e.preventDefault();
      forward();
    } else if (e.key === "Escape" && el.classList.contains("fullscreen")) {
      e.preventDefault();
      exitFullscreen();
    }
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
    if (el.classList.contains("fullscreen")) {
      getCurrentWindow().setFullscreen(false).catch(() => {});
    }
    document.removeEventListener("keydown", onKey);
    unlistenResize?.();
    unlistenResize = null;
    clearTimeout(hideTimer);
    video.pause();
    sub?.destroy();
    sub = null;
    video.src = "";
    api.playerStop().catch(() => {});
  };
  el.querySelector<HTMLButtonElement>(".back")!.onclick = () => { cleanup(); onExit(); };
  el.addEventListener("player-detach", cleanup);
  return el;
}
