import { api } from "../lib/ipc";
import type { MediaItem } from "../lib/ipc";
import { icon } from "../lib/icons";
import { esc } from "../lib/escape";
import { convertFileSrc } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
// 完整版 hls.js：精简版（hls.js/light）不含备选音轨支持，多音轨视频会没有声音也切不了音轨。
import Hls from "hls.js";
import { SubtitleRenderer } from "../components/SubtitleRenderer";

// 打开流程的阶段（前 4 个由后端 `player-stage` 事件驱动，后 2 个由前端 hls.js 驱动）。
// 命中完整缓存时后端只报 check/probe，随后直接跳到 manifest。
const STAGES = [
  ["check", "检查播放组件"],
  ["probe", "读取影片信息"],
  ["transcode", "启动转码引擎"],
  ["segment", "生成首个片段"],
  ["manifest", "加载播放列表"],
  ["buffer", "缓冲画面"],
] as const;
type Stage = (typeof STAGES)[number][0];

const HLS_ERR: Record<string, string> = {
  manifestLoadError: "播放列表加载失败",
  manifestLoadTimeOut: "播放列表加载超时",
  manifestParsingError: "播放列表格式错误",
  levelLoadError: "播放列表刷新失败",
  fragLoadError: "视频片段加载失败",
  fragLoadTimeOut: "视频片段加载超时",
  bufferAppendError: "视频片段解码失败",
  audioTrackLoadError: "音轨加载失败",
  audioTrackLoadTimeOut: "音轨加载超时",
};

export async function PlayerView(it: MediaItem, onExit: () => void): Promise<HTMLElement> {
  const el = document.createElement("div");
  el.className = "player-view view-enter";
  el.innerHTML = `
    <div class="player-top">
      <button class="back icon-text">${icon("arrowLeft", 16)}<span>返回</span></button>
      <span class="player-title">${esc(it.title)}</span>
      <span class="player-cache" style="display:none"></span>
    </div>
    <div class="player-stage-wrap">
      <div class="player-loading loading-box">
        <div class="spinner"></div>
        <div class="player-loading-text">准备中…</div>
        <div class="player-progress"><div class="player-progress-fill"></div></div>
        <div class="player-loading-step"></div>
        <div class="player-loading-hint"></div>
        <button class="player-retry icon-text" style="display:none">重试</button>
      </div>
      <div class="player-video-box">
        <video class="player-video" playsinline style="display:none"></video>
        <div class="player-buffering"><div class="spinner"></div><span class="player-buffering-text">缓冲中…</span></div>
      </div>
    </div>
    <div class="player-bar glass">
      <button class="pp">${icon("pause", 18)}</button>
      <button class="rw">${icon("rewind", 18)}</button>
      <button class="ff">${icon("forward", 18)}</button>
      <div class="seek-wrap">
        <input class="seek" type="range" min="0" max="1000" value="0"/>
        <div class="seek-cache" title="已缓存范围"><div class="seek-cache-fill"></div></div>
      </div>
      <span class="time">0:00 / 0:00</span>
      <input class="vol" type="range" min="0" max="100" value="100"/>
      <div class="player-tools">
        <div class="audio-wrap" style="display:none">
          <button class="au" title="音轨">${icon("audioTrack", 18)}</button>
          <div class="audio-menu" hidden></div>
        </div>
        <button class="cc" title="字幕" style="display:none">${icon("captions", 18)}</button>
        <button class="fs">${icon("fullscreen", 18)}</button>
      </div>
    </div>`;

  const video = el.querySelector<HTMLVideoElement>(".player-video")!;
  const loading = el.querySelector<HTMLElement>(".player-loading")!;
  const loadingText = el.querySelector<HTMLElement>(".player-loading-text")!;
  const progressFill = el.querySelector<HTMLElement>(".player-progress-fill")!;
  const stepText = el.querySelector<HTMLElement>(".player-loading-step")!;
  const hintText = el.querySelector<HTMLElement>(".player-loading-hint")!;
  const retryBtn = el.querySelector<HTMLButtonElement>(".player-retry")!;
  const bufferingText = el.querySelector<HTMLElement>(".player-buffering-text")!;
  const cachePill = el.querySelector<HTMLElement>(".player-cache")!;
  const cacheBar = el.querySelector<HTMLElement>(".seek-cache")!;
  const cacheFill = el.querySelector<HTMLElement>(".seek-cache-fill")!;
  const seek = el.querySelector<HTMLInputElement>(".seek")!;
  const time = el.querySelector<HTMLElement>(".time")!;
  const fmt = (s: number) => `${Math.floor(s / 60)}:${String(Math.floor(s % 60)).padStart(2, "0")}`;
  let duration = 0;
  let seeking = false;
  let closed = false;
  let sub: SubtitleRenderer | null = null;
  let subtitleOn = true;
  let pendingSubUrl: string | null = null;
  let hls: Hls | null = null;
  const ccBtn = el.querySelector<HTMLButtonElement>(".cc")!;
  const dur = () => duration || video.duration || 0;

  // ===== 音轨切换（仅多音轨视频）=====
  // 后端把视频和每条音轨分成独立的 HLS 流，hls.js 切音轨时只换音频缓冲、视频不动，画面不卡。
  const audioWrap = el.querySelector<HTMLElement>(".audio-wrap")!;
  const audioBtn = el.querySelector<HTMLButtonElement>(".au")!;
  const audioMenu = el.querySelector<HTMLElement>(".audio-menu")!;
  // 记住每部视频上次选的音轨（按音轨名记，名字由后端保证同一视频内唯一）
  const audioPrefKey = `collector.audio:${it.category}/${it.category_path}/${it.title}`;
  const readAudioPref = (): string | null => {
    try { return localStorage.getItem(audioPrefKey); } catch { return null; }
  };
  const renderAudioMenu = () => {
    const tracks = hls?.audioTracks ?? [];
    const multi = tracks.length > 1;
    audioWrap.style.display = multi ? "" : "none";
    if (!multi) { audioMenu.hidden = true; audioMenu.innerHTML = ""; return; }
    const current = hls!.audioTrack;
    audioMenu.innerHTML = tracks
      .map((t, i) => `<button class="audio-item${i === current ? " on" : ""}" data-idx="${i}">${esc(t.name)}</button>`)
      .join("");
  };
  // 上次选的音轨主要靠 hls.js 的 audioPreference 在起播前选中（见 start()）；
  // 这里是兜底：音轨列表出来后若当前不是想要的那条，再切一次。每个 hls 实例只做一次，
  // 否则用户手动切走后又会被切回来。
  let audioPrefApplied = false;
  const applyAudioPref = () => {
    const h = hls;
    if (!h || audioPrefApplied || h.audioTracks.length === 0) return;
    audioPrefApplied = true;
    const want = readAudioPref();
    const idx = want ? h.audioTracks.findIndex((t) => t.name === want) : -1;
    if (idx >= 0 && idx !== h.audioTrack) h.audioTrack = idx;
  };
  const closeAudioMenu = () => { audioMenu.hidden = true; };
  audioBtn.onclick = (e) => {
    e.stopPropagation();
    audioMenu.hidden = !audioMenu.hidden;
  };
  audioMenu.onclick = (e) => {
    e.stopPropagation();
    const item = (e.target as HTMLElement).closest<HTMLElement>(".audio-item");
    const h = hls;
    if (!item || !h) return;
    const idx = Number(item.dataset.idx);
    const track = h.audioTracks[idx];
    if (!track) return;
    if (idx !== h.audioTrack) h.audioTrack = idx;
    try { localStorage.setItem(audioPrefKey, track.name); } catch { /* 存不了就不记 */ }
    closeAudioMenu();
    renderAudioMenu();
  };
  const onDocMouseDown = (e: MouseEvent) => {
    if (!audioMenu.hidden && !audioWrap.contains(e.target as Node)) closeAudioMenu();
  };
  document.addEventListener("mousedown", onDocMouseDown);

  // ===== 加载态：阶段文字 + 进度条 =====
  let stageIdx = -1;
  let failed = false;
  let ready = false;          // 已起播（首个 canplay 之后）
  let resumeAt = 0;           // 出错重试时从这里接着播
  let slowTimer: number | undefined;
  const setStage = (s: Stage) => {
    const i = STAGES.findIndex((x) => x[0] === s);
    if (closed || failed || ready || i <= stageIdx) return; // 只前进不回退
    stageIdx = i;
    loadingText.textContent = STAGES[i][1] + "…";
    stepText.textContent = `步骤 ${i + 1} / ${STAGES.length}`;
    progressFill.style.width = `${((i + 1) / STAGES.length) * 100}%`;
    hintText.textContent = "";
    clearTimeout(slowTimer);
    slowTimer = window.setTimeout(() => {
      if (!closed && !failed && !ready) hintText.textContent = "仍在处理，体积较大的影片首次打开会稍慢，请稍候…";
    }, 6000);
  };
  // 失败：把原因和「重试」摆出来。播放中途出错同样走这里（此前只改了隐藏着的文字 → 画面静止无提示）。
  const fail = (title: string, detail = "") => {
    if (closed) return;
    failed = true;
    clearTimeout(slowTimer);
    if (ready) resumeAt = video.currentTime;
    video.pause();
    setBuffering(false);
    video.style.display = "none";
    loading.style.display = "";
    loading.classList.add("is-error");
    loadingText.textContent = title;
    hintText.textContent = detail;
    retryBtn.style.display = "";
  };

  // ===== 播放中的缓冲提示 =====
  let transcoding = false;
  let transcodedSecs = 0;
  let bufferingTimer: number | undefined;
  const setBuffering = (on: boolean) => {
    clearTimeout(bufferingTimer);
    if (!on) { el.classList.remove("is-buffering"); return; }
    // 延迟 250ms 再显示，避免每次 seek 都闪一下
    bufferingTimer = window.setTimeout(() => {
      if (closed || failed || !ready) return;
      bufferingText.textContent = transcoding && video.currentTime > transcodedSecs
        ? "该位置仍在转码，就绪后自动播放…"
        : "缓冲中…";
      el.classList.add("is-buffering");
    }, 250);
  };
  video.addEventListener("waiting", () => setBuffering(true));
  video.addEventListener("seeking", () => setBuffering(true));
  for (const ev of ["playing", "seeked", "canplay"]) video.addEventListener(ev, () => setBuffering(false));

  // ===== 后台转码（缓存）进度 =====
  let currentSrc = "";
  let cacheHideTimer: number | undefined;
  const setCache = (seconds: number, done: boolean) => {
    transcodedSecs = seconds;
    transcoding = !done;
    const d = dur();
    const pct = done ? 100 : d > 0 ? Math.min(99, (seconds / d) * 100) : 0;
    cacheFill.style.width = `${pct}%`;
    cacheBar.style.display = done ? "none" : "";
    clearTimeout(cacheHideTimer);
    if (done) {
      // 仅当之前显示过"缓存中"才提示完成；完整缓存直接打开时不打扰
      if (cachePill.style.display !== "none") {
        cachePill.textContent = "缓存完成";
        cachePill.classList.add("done");
        cacheHideTimer = window.setTimeout(() => { cachePill.style.display = "none"; }, 2500);
      }
    } else {
      cachePill.classList.remove("done");
      cachePill.textContent = `后台缓存中 ${Math.floor(pct)}%`;
      cachePill.style.display = "";
    }
  };

  // 后端事件：打开阶段 + 转码进度。先注册好再调 playerOpen，避免漏掉最早的阶段事件。
  const unlisteners: (() => void)[] = [];
  const listenersReady = Promise.all([
    listen<string>("player-stage", (e) => setStage(e.payload as Stage)),
    listen<{ src: string; seconds: number; done: boolean }>("player-transcode", (e) => {
      if (closed || e.payload.src !== currentSrc) return;
      setCache(e.payload.seconds, e.payload.done);
    }),
  ]).then((uns) => {
    if (closed) uns.forEach((un) => un());
    else unlisteners.push(...uns);
  }).catch((e) => console.error("[player] listen failed", e));

  // 播放 HLS：Chromium 无原生 HLS，通过 hls.js 走 MSE。首段（~2s）就绪即 canplay。
  // video 加载/解码错误 → 显示具体原因
  const MEDIA_ERR: Record<number, string> = {
    1: "加载被中止",
    2: "网络错误",
    3: "解码失败（编码不受支持）",
    4: "源不可用或格式不支持",
  };
  video.addEventListener("error", () => {
    if (closed || !video.error) return;
    const code = video.error.code;
    console.error("[player] video error", code, video.error.message, "src=", video.src);
    fail("视频加载失败", MEDIA_ERR[code] || `未知错误(${code})`);
  });
  // 能播放了才隐藏 loading、显示 video
  video.addEventListener("canplay", () => {
    if (closed || failed || ready) return;
    ready = true;
    clearTimeout(slowTimer);
    loading.style.display = "none";
    video.style.display = "";
    video.play().catch(() => {});
    // 字幕初始化（libass wasm）较重，推迟到视频已起播且主线程空闲时再挂载，
    // 避免其首次初始化阻塞起播关键路径造成"打开时卡顿"。
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
      if (!video.paused) schedule();
      else video.addEventListener("playing", schedule, { once: true });
    }
  });

  // 打开（或重试）：不 await——视图先挂上去，加载进度才看得见。
  const start = async () => {
    failed = false;
    ready = false;
    stageIdx = -1;
    hls?.destroy();
    hls = null;
    renderAudioMenu(); // 旧实例已销毁：先收起音轨按钮，新清单就绪后再显示
    loading.classList.remove("is-error");
    retryBtn.style.display = "none";
    loading.style.display = "";
    video.style.display = "none";
    setStage("check");
    try {
      await listenersReady;
      const info = await api.playerOpen(it.category, it.category_path, it.title);
      // 打开途中用户已返回：后端此刻才登记会话，补一次 stop，别让 ffmpeg 白跑
      if (closed) { api.playerStop().catch(() => {}); return; }
      duration = info.duration;
      currentSrc = info.src;
      if (info.subtitle) pendingSubUrl = convertFileSrc(info.subtitle);
      setCache(info.transcoded, !info.transcoding);

      // 起播：把 HLS playlist URL 交给 hls.js（走自定义 `hls://` 协议——见 lib.rs 的
      // register_uri_scheme_protocol("hls")）。fetch 到该协议走 Tauri 的 URI handler，
      // 完全绕开 asset 协议对 fetch 的 404 限制、也绕开 WebView2 对 http://localhost:PORT 的
      // fetch 拦截。相对段 URL 由 hls.js 自动拼在 manifest URL 上（同源、同 path 前缀）。
      //
      // info.src 是文件名（如 `collector_HASH.m3u8`），拼成 `http://hls.localhost/文件名`。
      // 段 URL: hls.js 见到 EXTINF 后一行 `HASH_00000.ts`，`new URL(seg, manifest)` →
      // `http://hls.localhost/HASH_00000.ts` — 由同个 URI handler 服务。
      const manifestSrc = navigator.userAgent.includes("Windows")
        ? `http://hls.localhost/${info.src}`
        : `hls://localhost/${info.src}`;
      setStage("manifest");
      if (Hls.isSupported()) {
        // startPosition：转码进行中的 playlist 没有 ENDLIST，hls.js 会当作直播、
        // 默认从"直播边缘"（最新几段）起播——必须显式指定从头（或重试前的位置）开始。
        // audioPreference：上次选过的音轨在起播前就选中，不会先加载默认音轨再跳过去。
        const audioPref = readAudioPref();
        audioPrefApplied = false;
        const h = new Hls({
          startPosition: resumeAt,
          ...(audioPref ? { audioPreference: { name: audioPref } } : {}),
        });
        hls = h;
        let netRetries = 0;
        let mediaRetries = 0;
        h.on(Hls.Events.MANIFEST_PARSED, () => setStage("buffer"));
        h.on(Hls.Events.AUDIO_TRACKS_UPDATED, () => {
          if (hls !== h) return;
          applyAudioPref();
          renderAudioMenu();
        });
        h.on(Hls.Events.AUDIO_TRACK_SWITCHED, () => { if (hls === h) renderAudioMenu(); });
        h.on(Hls.Events.ERROR, (_event, data) => {
          if (closed || hls !== h || !data.fatal) return;
          console.error("[player] hls fatal", {
            type: data.type,
            details: data.details,
            reason: data.reason,
            response: data.response,
            url: data.url,
            err: data.err && String(data.err),
          });
          // 瞬时故障自动恢复：网络类重试 3 次（ffmpeg 正在改写文件时偶发读失败），解码类恢复 2 次
          if (data.type === Hls.ErrorTypes.NETWORK_ERROR && netRetries < 3) {
            netRetries++;
            const manifestFailed = data.details === Hls.ErrorDetails.MANIFEST_LOAD_ERROR
              || data.details === Hls.ErrorDetails.MANIFEST_LOAD_TIMEOUT;
            window.setTimeout(() => {
              if (closed || hls !== h) return;
              if (manifestFailed) h.loadSource(manifestSrc);
              else h.startLoad();
            }, 400 * netRetries);
            return;
          }
          if (data.type === Hls.ErrorTypes.MEDIA_ERROR && mediaRetries < 2) {
            mediaRetries++;
            h.recoverMediaError();
            return;
          }
          fail("视频加载失败", HLS_ERR[data.details] || data.details || data.type);
        });
        h.loadSource(manifestSrc);
        h.attachMedia(video);
      } else if (video.canPlayType("application/vnd.apple.mpegurl")) {
        // Safari/WKWebView 原生支持 HLS（macOS 场景兜底）
        video.src = manifestSrc;
        video.load();
        setStage("buffer");
      } else {
        throw new Error("浏览器既不支持 MSE 也不支持原生 HLS");
      }
    } catch (e) {
      console.error("[player] playerOpen failed", e);
      fail("无法播放该视频", typeof e === "string" ? e : (e as any)?.message ?? String(e));
    }
  };
  retryBtn.onclick = () => { start(); };
  start();

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
      hideTimer = window.setTimeout(() => {
        // 音轨菜单开着时不隐藏工具栏（否则菜单会跟着消失），稍后再看
        if (!audioMenu.hidden) { showControls(); return; }
        el.classList.add("controls-hidden");
      }, 1500);
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
    } else if (e.key === "Escape" && !audioMenu.hidden) {
      e.preventDefault();
      closeAudioMenu();
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
    document.removeEventListener("mousedown", onDocMouseDown);
    unlistenResize?.();
    unlistenResize = null;
    unlisteners.forEach((un) => un());
    unlisteners.length = 0;
    clearTimeout(hideTimer);
    clearTimeout(slowTimer);
    clearTimeout(bufferingTimer);
    clearTimeout(cacheHideTimer);
    video.pause();
    sub?.destroy();
    sub = null;
    hls?.destroy();
    hls = null;
    video.src = "";
    api.playerStop().catch(() => {});
  };
  el.querySelector<HTMLButtonElement>(".back")!.onclick = () => { cleanup(); onExit(); };
  el.addEventListener("player-detach", cleanup);
  return el;
}
