/// <reference types="vite/client" />

declare const __APP_VERSION__: string;

// hls.js 的 light 构建（不含字幕轨/多音轨/DRM 等本应用用不到的模块，体积约小三分之一）
// 没有在 package.json exports 里声明类型；API 与完整版一致，直接复用完整版类型。
declare module "hls.js/light" {
  export * from "hls.js";
  export { default } from "hls.js";
}
