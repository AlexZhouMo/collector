// @ts-expect-error libass-wasm 无类型声明，构造函数在下方用最小接口约束
import SubtitlesOctopus from "libass-wasm";

// 打包的 CJK 兜底字体（保证中文不方框，跨平台一致）
const cjkFontUrl = new URL("../assets/fonts/NotoSansCJKsc-Regular.woff2", import.meta.url).href;
// libass worker + wasm 放在 public/libass/，Vite 原样服务、不 hash，worker 同目录能 locateFile wasm
const WORKER_URL = "/libass/subtitles-octopus-worker.js";
const LEGACY_WORKER_URL = "/libass/subtitles-octopus-worker-legacy.js";

/** SubtitlesOctopus 实例的最小接口（该库无 TS 类型）。 */
interface OctopusInstance {
  canvas?: HTMLCanvasElement;
  dispose: () => void;
}
interface OctopusOptions {
  video: HTMLVideoElement;
  subContent: string;
  workerUrl: string;
  legacyWorkerUrl: string;
  fonts?: string[];
  fallbackFont?: string;
  onReady?: () => void;
  onError?: (e: unknown) => void;
}
type OctopusCtor = new (opts: OctopusOptions) => OctopusInstance;

/**
 * 封装 SubtitlesOctopus（libass-wasm）渲染 .ass 字幕，时间轴跟随 <video> 自动同步。
 *
 * 为什么用 SubtitlesOctopus 而非 JASSUB：
 * JASSUB 唯一渲染路径是 worker + transferControlToOffscreen()，该合成在 Tauri 的
 * WKWebView 里不 present（经最小实验确认：worker 绘制到 transferred canvas 后画面不更新），
 * 导致字幕解析正常却完全不可见。SubtitlesOctopus 不用 OffscreenCanvas——字幕位图从
 * worker 传回主线程，主线程用 putImageData/drawImage 画到普通 canvas，且内置了 WebKit
 * 透明像素 bug 的 workaround，在 WKWebView 可靠显示。二者同为 libass，还原度一致。
 *
 * 初始化失败不抛出——仅 console.error，视频照常无字幕播放。
 */
export class SubtitleRenderer {
  private instance: OctopusInstance | null = null;
  private destroyed = false;

  constructor(video: HTMLVideoElement, subUrl: string) {
    // 主线程 fetch 字幕文本，用 subContent 传入（asset:// 在主线程可靠，worker 里未必）
    fetch(subUrl)
      .then((r) => r.text())
      .then((subContent) => {
        if (this.destroyed) return;
        this.init(video, subContent);
      })
      .catch((e) => console.error("[subtitle] 字幕内容加载失败", e));
  }

  private init(video: HTMLVideoElement, subContent: string): void {
    try {
      const Ctor = SubtitlesOctopus as unknown as OctopusCtor;
      this.instance = new Ctor({
        video,
        subContent,
        workerUrl: WORKER_URL,
        legacyWorkerUrl: LEGACY_WORKER_URL,
        // 兜底字体：字幕指名字体缺失时用它（中文不方框）
        fonts: [cjkFontUrl],
        fallbackFont: cjkFontUrl,
        onError: (e) => console.error("[subtitle] SubtitlesOctopus error", e),
      });
    } catch (e) {
      console.error("[subtitle] SubtitlesOctopus init failed", e);
      this.instance = null;
    }
  }

  /** 显示/隐藏字幕层（CC 开关）。 */
  setVisible(visible: boolean): void {
    if (!this.instance) return;
    try {
      const canvas = this.instance.canvas;
      if (canvas) canvas.style.display = visible ? "" : "none";
    } catch (e) {
      console.error("[subtitle] setVisible failed", e);
    }
  }

  /** 释放 worker/wasm/canvas（退出播放或换片必须调用）。 */
  destroy(): void {
    this.destroyed = true;
    if (!this.instance) return;
    try {
      this.instance.dispose();
    } catch (e) {
      console.error("[subtitle] destroy failed", e);
    }
    this.instance = null;
  }
}
