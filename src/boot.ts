// 启动入口：等后端后台初始化（首启释放 seed、打开数据库）完成后，再加载主界面模块，
// 期间显示 index.html 里的启动画面；就绪后淡出移除。初始化失败则在启动画面上显示原因。
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

/** 等待后端就绪：监听 app-init 事件，并定时轮询兜底（防止事件在监听前已发出）。 */
function waitReady(): Promise<void> {
  return new Promise((resolve, reject) => {
    let done = false;
    let unlisten: (() => void) | undefined;
    const finish = (err?: unknown) => {
      if (done) return;
      done = true;
      window.clearInterval(timer);
      unlisten?.();
      if (err === undefined) resolve(); else reject(err);
    };
    const check = () => {
      invoke<boolean>("app_ready").then((ok) => { if (ok) finish(); }, (e) => finish(e ?? "初始化失败"));
    };
    const timer = window.setInterval(check, 300);
    listen("app-init", check).then((u) => { if (done) u(); else unlisten = u; });
    check();
  });
}

function hideSplash() {
  const splash = document.getElementById("splash");
  if (!splash) return;
  splash.classList.add("out");
  const remove = () => splash.remove();
  splash.addEventListener("transitionend", remove, { once: true });
  window.setTimeout(remove, 800); // transitionend 未触发时兜底
}

function showInitError(err: unknown) {
  const msg = document.getElementById("splash-msg");
  if (msg) {
    msg.textContent = "初始化失败：" + (typeof err === "string" ? err : (err as Error)?.message ?? String(err));
    msg.classList.add("err");
  }
  document.querySelector(".sp-bar")?.classList.add("stop");
}

waitReady()
  .then(() => import("./main"))
  // 等两帧，让主界面完成首次绘制后再淡出，避免露出空白
  .then(() => requestAnimationFrame(() => requestAnimationFrame(hideSplash)))
  .catch(showInitError);
