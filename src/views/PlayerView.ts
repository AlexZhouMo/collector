import { api } from "../lib/ipc";
import type { MediaItem } from "../lib/ipc";
import { icon } from "../lib/icons";
import { esc } from "../lib/escape";
import { convertFileSrc } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { listen } from "@tauri-apps/api/event";
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
      <div class="player-loading loading-box">
        <div class="spinner"></div>
        <div class="player-loading-text">准备中…</div>
      </div>
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
  const loadingText = el.querySelector<HTMLElement>(".player-loading-text")!;
  const spinner = el.querySelector<HTMLElement>(".player-loading .spinner")!;
  const seek = el.querySelector<HTMLInputElement>(".seek")!;
  const time = el.querySelector<HTMLElement>(".time")!;
  const fmt = (s: number) => `${Math.floor(s / 60)}:${String(Math.floor(s % 60)).padStart(2, "0")}`;
  let duration = 0;
  let seeking = false;
  let closed = false;
  let sub: SubtitleRenderer | null = null;
  let subtitleOn = true;
  let pendingSubUrl: string | null = null;
  let unlistenProgress: (() => void) | null = null;
  // 起播状态（顶层，供进度回调与错误处理读取）
  let progressive = false;    // 是否需等转码完成再播
  let srcUrl = "";            // 视频源 URL
  let started = false;        // 是否已设过 video.src
  const ccBtn = el.querySelector<HTMLButtonElement>(".cc")!;
  const dur = () => duration || video.duration || 0;

  // 经本地 HTTP server 播放缓存 mp4：完整产物秒开复用，未缓存则边转边播（进度阈值起播）。
  // video 加载/解码错误 → 显示具体原因（诊断 + 用户提示）
  const MEDIA_ERR: Record<number, string> = {
    1: "加载被中止",
    2: "网络错误",
    3: "解码失败（编码不受支持）",
    4: "源不可用或格式不支持",
  };
  video.addEventListener("error", () => {
    if (closed) return;
    const code = video.error?.code ?? 0;
    const msg = MEDIA_ERR[code] || `未知错误(${code})`;
    loadingText.textContent = "视频加载失败：" + msg;
    spinner.style.display = "none";
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
    // 字幕初始化（libass wasm）较重，推迟到视频已起播且主线程空闲时再挂载，
    // 避免其首次初始化阻塞起播关键路径造成"打开时卡顿"。
    // libass 渲染器按 video 显示尺寸定位字幕 canvas，需 video 已可见（有尺寸）。
    if (pendingSubUrl && !sub) {
      const initSub = () => {
        if (closed || sub || !pendingSubUrl) return;
        sub = new SubtitleRenderer(video, pendingSubUrl);
        ccBtn.style.display = "";
        ccBtn.classList.add("cc-on");
      };
      const schedule = () => {
        if (typeof requestIdleCallback === "function") requestIdleCallback(initSub, { timeout: 1500 });
        else setTimeout(initSub, 300);
      };
      // 等真正开始播放再排期，进一步错开起播关键帧
      if (!video.paused) schedule();
      else video.addEventListener("playing", schedule, { once: true });
    }
  }, { once: true });

  try {
    loadingText.textContent = "准备中…（转封装视频）";
    // 首次打开较慢（libass 字幕组件冷启动等），超过 2.5s 仍未起播则给出"首次稍慢"提示，
    // 避免用户误以为卡死。起播（canplay 隐藏 loading）或出错时此提示自然随 loading 消失。
    const slowHintTimer = window.setTimeout(() => {
      if (!closed && loading.style.display !== "none") {
        loadingText.textContent = "首次打开需初始化，请稍候…";
      }
    }, 2500);
    el.addEventListener("player-detach", () => clearTimeout(slowHintTimer));

    // 先注册进度监听，再调 player_open——ffmpeg `-c:v copy` 极快，可能在 listen
    // 注册完成前就转完并 emit 完所有进度事件，导致前端漏掉事件、永不起播。
    // 监听器缓存最新事件；拿到 epoch 后用缓存补判起播（不依赖事件到达时序）。
    let wantEpoch = -1;
    type Prog = { epoch: number; ok_seconds: number; done: boolean; failed: boolean };
    let latest: Prog | null = null;
    const startPlayback = () => {
      if (started || closed || !srcUrl) return;
      started = true;
      loadingText.textContent = "加载中…";
      video.src = srcUrl;
      video.load();
    };
    const applyProgress = (p: Prog) => {
      if (closed || !progressive || p.epoch !== wantEpoch) return;
      if (p.failed) {
        loadingText.style.whiteSpace = "pre-line";
        spinner.style.display = "none";
        loadingText.textContent = "转码失败：该视频可能编码不受支持（当前仅支持 H.264）";
        return;
      }
      // WKWebView 不支持渐进解析，必须转码完成才播。转码中显示进度百分比。
      if (p.done) {
        loadingText.textContent = "即将播放…";
        startPlayback();
      } else if (duration > 0) {
        const pct = Math.min(99, Math.floor((p.ok_seconds / duration) * 100));
        loadingText.textContent = `转码中… ${pct}%`;
      } else {
        loadingText.textContent = "转码中…";
      }
    };
    unlistenProgress = await listen<Prog>("transcode-progress", (e) => {
      latest = e.payload;           // 缓存最新（即使 epoch 未定/不匹配，供拿到 epoch 后补判）
      applyProgress(e.payload);
    });
    if (closed) { unlistenProgress?.(); unlistenProgress = null; return el; }

    const info = await api.playerOpen(it.category, it.category_path, it.title);
    if (closed) { unlistenProgress?.(); unlistenProgress = null; return el; }
    duration = info.duration;
    srcUrl = info.src;
    // 记下字幕 URL，待 canplay（video 有尺寸）后再初始化字幕渲染器
    if (info.subtitle) pendingSubUrl = convertFileSrc(info.subtitle);

    if (!info.progressive) {
      // 完整产物秒开复用：直接播放，seek 不受限（无需进度监听）。
      unlistenProgress?.();
      unlistenProgress = null;
      startPlayback();
    } else {
      // 转码完成再播（WKWebView 不支持渐进解析 fmp4）：显示转码进度，done 后起播。
      // 用注册后已缓存的最新事件补判一次（覆盖 player_open 返回前事件已到达的竞态）。
      wantEpoch = info.epoch;
      progressive = true;
      loadingText.textContent = "转码中…";
      if (latest) applyProgress(latest);
    }
  } catch (e) {
    unlistenProgress?.();
    unlistenProgress = null;
    spinner.style.display = "none";
    loadingText.style.whiteSpace = "pre-line";
    loadingText.style.textAlign = "left";
    loadingText.style.maxWidth = "560px";
    loadingText.style.lineHeight = "1.6";
    loadingText.textContent = "无法播放该视频：\n\n" + e;
    console.error("[player] playerOpen failed", e);
  }

  const togglePlay = () => {
    if (video.paused) { video.play(); el.querySelector(".pp")!.innerHTML = icon("pause", 18); }
    else { video.pause(); el.querySelector(".pp")!.innerHTML = icon("play", 18); }
  };
  // 完整产物播放，seek 可达任意位置（钳到 [0, 时长]）。
  const clampSeek = (t: number) => Math.max(0, Math.min(t, dur()));
  const rewind = () => { video.currentTime = clampSeek(video.currentTime - 10); };
  const forward = () => { video.currentTime = clampSeek(video.currentTime + 10); };
  el.querySelector<HTMLButtonElement>(".pp")!.onclick = togglePlay;
  el.querySelector<HTMLButtonElement>(".rw")!.onclick = rewind;
  el.querySelector<HTMLButtonElement>(".ff")!.onclick = forward;
  el.querySelector<HTMLInputElement>(".vol")!.oninput = (e) =>
    video.volume = Number((e.target as HTMLInputElement).value) / 100;
  seek.oninput = () => { seeking = true; };
  seek.onchange = () => {
    video.currentTime = clampSeek((Number(seek.value) / 1000) * dur());
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
    unlistenProgress?.();
    unlistenProgress = null;
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
