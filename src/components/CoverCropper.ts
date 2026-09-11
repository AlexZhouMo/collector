import { api } from "../lib/ipc";
import { convertFileSrc } from "@tauri-apps/api/core";

/** 裁剪框的屏幕矩形（相对图片显示区左上角，单位 px）。 */
export interface ScreenRect { left: number; top: number; width: number; height: number; }

/**
 * 把裁剪框的屏幕坐标换算为原图像素坐标 (x,y,w,h)，并 clamp 到图片范围内。
 * scale = 原图像素 / 显示像素（naturalWidth / clientWidth）。
 * 纯函数，便于单测。
 */
export function screenRectToImageRect(
  rect: ScreenRect, scale: number, naturalW: number, naturalH: number
): { x: number; y: number; w: number; h: number } {
  let x = Math.round(rect.left * scale);
  let y = Math.round(rect.top * scale);
  let w = Math.round(rect.width * scale);
  let h = Math.round(rect.height * scale);
  x = Math.max(0, Math.min(x, naturalW));
  y = Math.max(0, Math.min(y, naturalH));
  w = Math.max(1, Math.min(w, naturalW - x));
  h = Math.max(1, Math.min(h, naturalH - y));
  return { x, y, w, h };
}

const RATIO = 2 / 3; // 海报宽:高 = 2:3

/**
 * 打开封面裁剪弹窗：显示原图 + 2:3 锁定裁剪框，用户拖动/缩放框选定区域，
 * 确定后调后端 import_cover_cropped 生成标准海报，回调 onDone(封面路径)。
 */
export function openCoverCropper(srcPath: string, onDone: (coverPath: string) => void, kind: "video" | "comic" = "video"): void {
  const overlay = document.createElement("div");
  overlay.className = "drawer-overlay";
  overlay.style.zIndex = "200";

  const modal = document.createElement("div");
  modal.className = "glass cropper-modal";
  modal.innerHTML = `
    <h3 style="font-size:15px;color:var(--text);margin-bottom:10px">裁剪封面（2:3）</h3>
    <div class="cropper-stage">
      <img class="cropper-img" alt="" />
      <div class="cropper-box"><div class="cropper-handle"></div></div>
    </div>
    <div class="cropper-actions">
      <button type="button" data-b="ok" class="btn-primary">确定</button>
      <button type="button" data-b="cancel">取消</button>
    </div>`;

  const img = modal.querySelector<HTMLImageElement>(".cropper-img")!;
  const box = modal.querySelector<HTMLElement>(".cropper-box")!;
  const handle = modal.querySelector<HTMLElement>(".cropper-handle")!;

  const close = () => {
    overlay.remove(); modal.remove();
    document.removeEventListener("keydown", onKey, true);
  };
  const onKey = (e: KeyboardEvent) => { if (e.key === "Escape") close(); };
  overlay.onclick = () => close();
  document.addEventListener("keydown", onKey, true);
  modal.querySelector<HTMLButtonElement>('[data-b="cancel"]')!.onclick = () => close();

  // 裁剪框当前位置/尺寸（相对图片显示区，px）
  let bx = 0, by = 0, bw = 0, bh = 0;
  const applyBox = () => {
    box.style.left = `${bx}px`; box.style.top = `${by}px`;
    box.style.width = `${bw}px`; box.style.height = `${bh}px`;
  };

  img.onload = () => {
    const iw = img.clientWidth, ih = img.clientHeight;
    // 初始框：在图内尽量大、居中、比例 2:3
    if (iw / ih > RATIO) { bh = ih; bw = ih * RATIO; } else { bw = iw; bh = iw / RATIO; }
    bx = (iw - bw) / 2; by = (ih - bh) / 2;
    applyBox();
  };
  img.src = convertFileSrc(srcPath);

  // 拖动框体
  box.onpointerdown = (e) => {
    if (e.target === handle) return; // 交给缩放
    e.preventDefault();
    const sx = e.clientX, sy = e.clientY, ox = bx, oy = by;
    const iw = img.clientWidth, ih = img.clientHeight;
    const move = (ev: PointerEvent) => {
      bx = Math.max(0, Math.min(ox + (ev.clientX - sx), iw - bw));
      by = Math.max(0, Math.min(oy + (ev.clientY - sy), ih - bh));
      applyBox();
    };
    const up = () => { document.removeEventListener("pointermove", move); document.removeEventListener("pointerup", up); };
    document.addEventListener("pointermove", move);
    document.addEventListener("pointerup", up);
  };

  // 右下角缩放（比例锁 2:3）
  handle.onpointerdown = (e) => {
    e.preventDefault(); e.stopPropagation();
    const sx = e.clientX, ow = bw;
    const iw = img.clientWidth, ih = img.clientHeight;
    const move = (ev: PointerEvent) => {
      let nw = Math.max(40, ow + (ev.clientX - sx));
      let nh = nw / RATIO;
      // 不超出图片边界
      if (bx + nw > iw) { nw = iw - bx; nh = nw / RATIO; }
      if (by + nh > ih) { nh = ih - by; nw = nh * RATIO; }
      bw = nw; bh = nh; applyBox();
    };
    const up = () => { document.removeEventListener("pointermove", move); document.removeEventListener("pointerup", up); };
    document.addEventListener("pointermove", move);
    document.addEventListener("pointerup", up);
  };

  // 确定：换算原图坐标 → 后端生成
  modal.querySelector<HTMLButtonElement>('[data-b="ok"]')!.onclick = async () => {
    const scale = img.naturalWidth / img.clientWidth;
    const rect = screenRectToImageRect(
      { left: bx, top: by, width: bw, height: bh }, scale, img.naturalWidth, img.naturalHeight
    );
    try {
      const cover = await api.importCoverCropped(srcPath, rect.x, rect.y, rect.w, rect.h, kind);
      close();
      onDone(cover);
    } catch (err) {
      alert("生成封面失败：" + String(err));
    }
  };

  document.body.appendChild(overlay);
  document.body.appendChild(modal);
}
