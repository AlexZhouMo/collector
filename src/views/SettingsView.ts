import { open } from "@tauri-apps/plugin-dialog";
import { api } from "../lib/ipc";

export async function SettingsView(): Promise<HTMLElement> {
  const el = document.createElement("div");
  el.className = "view-enter";
  const kinds: [string, string][] = [["video", "视频"], ["comic", "漫画"], ["game", "游戏"]];
  const roots = Object.fromEntries(await Promise.all(
    kinds.map(async ([k]) => [k, await api.getRoot(k)])));
  el.innerHTML = `<h1 style="font-size:20px;margin-bottom:16px">设置</h1>` +
    kinds.map(([k, label]) => `
      <div class="glass" style="padding:14px;margin-bottom:12px">
        <div style="margin-bottom:8px">${label}根目录：<span id="root-${k}" style="color:var(--text-dim)">${roots[k] ?? "未设置"}</span></div>
        <button data-pick="${k}">选择目录</button>
        <button data-scan="${k}">扫描</button>
      </div>`).join("");
  el.querySelectorAll<HTMLButtonElement>("[data-pick]").forEach(b => b.onclick = async () => {
    const k = b.dataset.pick!;
    const dir = await open({ directory: true });
    if (typeof dir === "string") { await api.setRoot(k, dir); el.querySelector(`#root-${k}`)!.textContent = dir; }
  });
  el.querySelectorAll<HTMLButtonElement>("[data-scan]").forEach(b => b.onclick = async () => {
    const n = await api.scanRoot(b.dataset.scan!);
    alert(`扫描完成，${n} 项`);
  });
  return el;
}
