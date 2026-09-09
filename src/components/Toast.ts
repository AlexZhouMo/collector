/**
 * 屏幕底部居中的轻提示。同一时刻只保留一个（新 toast 先移除旧的）。
 * 2.5s 后淡出并移除。kind="error" 用红色变体。
 */
export function showToast(message: string, kind: "info" | "error" = "info"): void {
  document.querySelectorAll(".toast").forEach((t) => t.remove());
  const el = document.createElement("div");
  el.className = "toast glass" + (kind === "error" ? " toast-error" : "");
  el.textContent = message;
  document.body.appendChild(el);
  requestAnimationFrame(() => el.classList.add("toast-show"));
  setTimeout(() => {
    el.classList.remove("toast-show");
    setTimeout(() => el.remove(), 300);
  }, 2500);
}
