import JASSUB from "jassub";
// Vite 资源导入：worker 与 wasm 由 Vite 打包并给出可访问 URL。
// 注意 jassub 2.5.16 的 dist 布局：worker 入口在 dist/worker/worker.js，
// wasm 在 dist/wasm/ 下（已用 node_modules 实际结构核实）。
import workerUrl from "jassub/dist/worker/worker.js?worker&url";
import wasmUrl from "jassub/dist/wasm/jassub-worker.wasm?url";
import modernWasmUrl from "jassub/dist/wasm/jassub-worker-modern.wasm?url";

// 打包的 CJK 兜底字体（保证中文不方框，跨平台一致）
const cjkFontUrl = new URL("../assets/fonts/NotoSansCJKsc-Regular.woff2", import.meta.url).href;
const CJK_FONT_FAMILY = "Noto Sans CJK SC";

/**
 * 封装 JASSUB：在给定 <video> 上叠加 canvas 渲染 .ass 字幕，时间轴由 JASSUB 内部
 * 跟随 video.currentTime 同步（seek/暂停/倍速自动跟随）。
 * 初始化失败不抛出——仅 console.error，视频照常无字幕播放。
 */
export class SubtitleRenderer {
  private instance: JASSUB | null = null;

  constructor(video: HTMLVideoElement, subUrl: string) {
    try {
      this.instance = new JASSUB({
        video,
        subUrl,
        workerUrl,
        wasmUrl,
        modernWasmUrl,
        // 兜底字体：libass 找不到字幕指名字体时用它（中文不方框）
        availableFonts: { [CJK_FONT_FAMILY.toLowerCase()]: cjkFontUrl },
        defaultFont: CJK_FONT_FAMILY,
      });
    } catch (e) {
      console.error("[subtitle] JASSUB init failed", e);
      this.instance = null;
    }
  }

  /** 显示/隐藏字幕层（CC 开关）。 */
  setVisible(visible: boolean): void {
    if (!this.instance) return;
    try {
      // JASSUB 把渲染 canvas 挂在 _canvas（已核实 jassub.d.ts：无公开 canvas 属性，
      // 私有字段为 _canvas: HTMLCanvasElement）。用它的 display 控制显隐。
      const canvas = (this.instance as unknown as { _canvas?: HTMLCanvasElement })._canvas;
      if (canvas) canvas.style.display = visible ? "" : "none";
    } catch (e) {
      console.error("[subtitle] setVisible failed", e);
    }
  }

  /** 释放 worker/wasm/canvas（退出播放或换片必须调用）。 */
  destroy(): void {
    if (!this.instance) return;
    try {
      this.instance.destroy();
    } catch (e) {
      console.error("[subtitle] destroy failed", e);
    }
    this.instance = null;
  }
}
