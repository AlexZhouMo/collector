import { open, save } from "@tauri-apps/plugin-dialog";
import { api } from "../lib/ipc";
import type { SubReport } from "../lib/ipc";
import { esc } from "../lib/escape";
import { icon } from "../lib/icons";

export function NormalizeView(): HTMLElement {
  const el = document.createElement("div");
  el.className = "view-enter";
  el.innerHTML = `
    <h1 style="font-size:20px;margin-bottom:16px">标准化工具台</h1>
    <div class="glass" style="padding:16px;margin-bottom:16px">
      <h3 style="margin-bottom:10px">字幕标准化</h3>
      <div style="display:flex;gap:10px;flex-wrap:wrap;align-items:center">
        <button class="icon-text" id="sub-in">${icon("folder", 15)}<span class="btn-label">选择输入目录</span></button><span id="sub-in-p" style="color:var(--text-dim)">未选</span>
        <button class="icon-text" id="sub-out">${icon("folder", 15)}<span class="btn-label">选择输出目录</span></button><span id="sub-out-p" style="color:var(--text-dim)">未选</span>
        <button class="icon-text" id="sub-run">${icon("play", 15)}<span class="btn-label">开始</span></button>
      </div>
      <div id="sub-report" style="margin-top:12px"></div>
    </div>
    <div class="glass" style="padding:16px">
      <h3 style="margin-bottom:10px">漫画标准化</h3>
      <div style="display:flex;gap:10px;flex-wrap:wrap;align-items:center">
        <button class="icon-text" id="c-dir">${icon("folder", 15)}<span class="btn-label">选择图片目录</span></button><span id="c-dir-p" style="color:var(--text-dim)">未选</span>
        <input id="c-prefix" placeholder="命名前缀，如 海贼王01" style="padding:6px"/>
        <button class="icon-text" id="c-out">${icon("folder", 15)}<span class="btn-label">选择输出zip</span></button><span id="c-out-p" style="color:var(--text-dim)">未选</span>
        <button class="icon-text" id="c-run">${icon("play", 15)}<span class="btn-label">开始</span></button>
      </div>
      <div id="c-report" style="margin-top:12px;color:var(--text-dim)"></div>
    </div>`;

  let subIn = "", subOut = "", cDir = "", cOut = "";
  const pick = async (setter: (v: string) => void, spanId: string) => {
    const d = await open({ directory: true });
    if (typeof d === "string") { setter(d); el.querySelector(`#${spanId}`)!.textContent = d; }
  };
  el.querySelector<HTMLButtonElement>("#sub-in")!.onclick = () => pick(v => subIn = v, "sub-in-p");
  el.querySelector<HTMLButtonElement>("#sub-out")!.onclick = () => pick(v => subOut = v, "sub-out-p");
  el.querySelector<HTMLButtonElement>("#c-dir")!.onclick = () => pick(v => cDir = v, "c-dir-p");
  el.querySelector<HTMLButtonElement>("#c-out")!.onclick = async () => {
    const f = await save({ filters: [{ name: "zip", extensions: ["zip"] }] });
    if (f) { cOut = f; el.querySelector("#c-out-p")!.textContent = f; }
  };

  el.querySelector<HTMLButtonElement>("#sub-run")!.onclick = async () => {
    if (!subIn || !subOut) { alert("请选择输入/输出目录"); return; }
    const btn = el.querySelector<HTMLButtonElement>("#sub-run")!;
    const label = btn.querySelector<HTMLElement>(".btn-label")!;
    btn.disabled = true; label.textContent = "处理中…";
    try {
      const reports: SubReport[] = await api.normalizeSubtitles(subIn, subOut);
      const totalIssues = reports.reduce((a, r) => a + r.issues.length, 0);
      // 报告折叠：先显示汇总，每个有问题的文件默认折叠，点击展开
      const box = el.querySelector("#sub-report")!;
      box.innerHTML = `<div style="margin-bottom:8px">处理 ${reports.length} 个文件，质检提示共 ${totalIssues} 条</div>`;
      reports.filter(r => r.issues.length).forEach(r => {
        const det = document.createElement("details");
        det.className = "glass";
        det.style.cssText = "padding:8px;margin-bottom:6px";
        const summary = document.createElement("summary");
        summary.style.cssText = "cursor:pointer";
        summary.textContent = `${r.file}（${r.issues.length} 条）`;
        det.appendChild(summary);
        const inner = document.createElement("div");
        inner.innerHTML = r.issues.map(i =>
          `<div style="color:#ffb08a;font-size:12px">L${i.line} [${esc(i.kind)}] ${esc(i.text)}</div>`).join("");
        det.appendChild(inner);
        box.appendChild(det);
      });
      if (totalIssues === 0) box.innerHTML += `<div style="color:#8fdca0">无质检问题</div>`;
    } catch (e) {
      alert("字幕标准化失败：" + e);
    } finally {
      btn.disabled = false; label.textContent = "开始";
    }
  };
  el.querySelector<HTMLButtonElement>("#c-run")!.onclick = async () => {
    const prefix = (el.querySelector("#c-prefix") as HTMLInputElement).value.trim();
    if (!cDir || !cOut || !prefix) { alert("请选择目录、前缀和输出zip"); return; }
    const btn = el.querySelector<HTMLButtonElement>("#c-run")!;
    const label = btn.querySelector<HTMLElement>(".btn-label")!;
    btn.disabled = true; label.textContent = "处理中…";
    try {
      const n = await api.normalizeComic(cDir, prefix, cOut);
      el.querySelector("#c-report")!.textContent = `完成：${n} 页已打包`;
    } catch (e) {
      el.querySelector("#c-report")!.textContent = "漫画标准化失败：" + e;
    } finally {
      btn.disabled = false; label.textContent = "开始";
    }
  };
  return el;
}
