import { open } from "@tauri-apps/plugin-dialog";
import { api } from "../lib/ipc";
import { icon } from "../lib/icons";
import {
  getState, subscribe,
  startPoster, startSubtitle, startComic,
  type TaskKey,
} from "../lib/normalizeStore";

// 工具箱：三个长任务的运行态/进度/结果都在 normalizeStore（模块级单例），
// 本视图只负责「渲染 + 从 store 回填」。切菜单销毁本视图不影响任务；切回时
// 重新构造视图并 syncFromStore 续显当前进度/结果。

export function NormalizeView(): HTMLElement {
  const el = document.createElement("div");
  el.className = "view-enter";
  el.innerHTML = `
    <h1 style="font-size:20px;margin-bottom:16px">工具箱</h1>
    <div class="glass setting-card">
      <div class="setting-card-head"><span class="setting-card-title">影视海报生成</span></div>
      <div class="setting-row">
        <span class="setting-label">TMDB Key</span>
        <input id="tmdb-key" class="setting-path" style="flex:1;padding:6px 10px;border-radius:8px;background:var(--glass);border:1px solid var(--border);color:var(--text)" placeholder="填入 TMDB API Key" />
      </div>
      <div class="setting-actions">
        <button class="btn-primary icon-text" id="fetch-posters">${icon("refresh", 15)}<span class="btn-label">更新海报</span></button>
      </div>
      <div id="poster-progress" style="display:none;margin-top:10px">
        <div style="height:6px;border-radius:4px;background:var(--glass);overflow:hidden">
          <div id="poster-progress-bar" style="height:100%;width:0%;background:var(--accent);transition:width .2s"></div>
        </div>
        <div id="poster-progress-text" style="font-size:12px;color:var(--text-dim);margin-top:4px"></div>
      </div>
      <div id="poster-result" style="margin-top:10px"></div>
    </div>
    <div class="glass setting-card">
      <div class="setting-card-head"><span class="setting-card-title">字幕批量校准</span></div>
      <div class="setting-row">
        <span class="setting-label">输入目录</span>
        <span id="sub-in-p" class="setting-path">docs/subtitles</span>
        <button class="icon-text" id="sub-in">${icon("folder", 15)}<span class="btn-label">选择</span></button>
      </div>
      <div class="setting-row">
        <span class="setting-label">输出</span>
        <span class="setting-path">应用字幕库（自动，按目录结构）</span>
      </div>
      <div class="setting-actions">
        <button class="btn-primary icon-text" id="sub-run" disabled>${icon("play", 15)}<span class="btn-label">开始校准</span></button>
      </div>
      <div id="sub-progress" style="display:none;margin-top:10px">
        <div style="height:6px;border-radius:4px;background:var(--glass);overflow:hidden">
          <div id="sub-progress-bar" style="height:100%;width:0%;background:var(--accent);transition:width .2s"></div>
        </div>
        <div id="sub-progress-text" style="font-size:12px;color:var(--text-dim);margin-top:4px"></div>
      </div>
      <div id="sub-report" style="margin-top:10px"></div>
    </div>
    <div class="glass setting-card">
      <div class="setting-card-head"><span class="setting-card-title">漫画自动归档</span></div>
      <div class="setting-row">
        <span class="setting-label">漫画目录</span>
        <span id="comic-dir-p" class="setting-path">未配置</span>
      </div>
      <div class="setting-actions">
        <button class="btn-primary icon-text" id="comic-run" disabled>${icon("archive", 15)}<span class="btn-label">开始归档</span></button>
      </div>
      <div id="comic-progress" style="display:none;margin-top:10px">
        <div style="height:6px;border-radius:4px;background:var(--glass);overflow:hidden">
          <div id="comic-progress-bar" style="height:100%;width:0%;background:var(--accent);transition:width .2s"></div>
        </div>
        <div id="comic-progress-text" style="font-size:12px;color:var(--text-dim);margin-top:4px"></div>
      </div>
      <div id="comic-report" style="margin-top:10px"></div>
    </div>`;

  // 配置就绪与否（与任务运行态一起决定 sub/comic 按钮是否可点）
  let subIn = "";
  let subConfigured = false;
  let comicConfigured = false;

  const $ = <T extends HTMLElement>(sel: string) => el.querySelector<T>(sel)!;

  // 三任务：按钮/进度容器/进度条/文字/结果区，及 running 时的按钮文案
  const specs: { key: TaskKey; btn: string; prog: string; bar: string; text: string; result: string; idle: string; busy: string; enabled: () => boolean }[] = [
    { key: "poster", btn: "#fetch-posters", prog: "#poster-progress", bar: "#poster-progress-bar", text: "#poster-progress-text", result: "#poster-result", idle: "更新海报", busy: "抓取中…", enabled: () => true },
    { key: "subtitle", btn: "#sub-run", prog: "#sub-progress", bar: "#sub-progress-bar", text: "#sub-progress-text", result: "#sub-report", idle: "开始校准", busy: "处理中…", enabled: () => subConfigured },
    { key: "comic", btn: "#comic-run", prog: "#comic-progress", bar: "#comic-progress-bar", text: "#comic-progress-text", result: "#comic-report", idle: "开始归档", busy: "归档中…", enabled: () => comicConfigured },
  ];

  // 从 store 回填一个任务的进度条/文字/结果/按钮态
  const syncTask = (s: typeof specs[number]) => {
    const st = getState(s.key);
    const btn = $<HTMLButtonElement>(s.btn);
    const label = btn.querySelector<HTMLElement>(".btn-label")!;
    const running = st.status === "running";
    btn.disabled = running || !s.enabled();
    label.textContent = running ? s.busy : s.idle;
    const prog = $<HTMLElement>(s.prog);
    if (st.status === "idle") {
      prog.style.display = "none";
    } else {
      prog.style.display = "block";
      $<HTMLElement>(s.bar).style.width = st.pct + "%";
      $<HTMLElement>(s.text).textContent = st.statusText;
    }
    $<HTMLElement>(s.result).innerHTML = st.resultHtml;
  };
  const syncFromStore = () => specs.forEach(syncTask);

  // 配置读取（与 store 无关）：字幕输入目录、漫画根目录、TMDB Key
  api.getSubtitleInputDir().then((d) => {
    if (d) {
      subIn = d; subConfigured = true;
      $("#sub-in-p").textContent = d;
      syncTask(specs[1]);
    }
  });
  api.getRoot("comic").then((d) => {
    if (d) {
      comicConfigured = true;
      $("#comic-dir-p").textContent = d;
      syncTask(specs[2]);
    } else {
      $("#comic-dir-p").textContent = "请先在设置中配置漫画根目录";
    }
  });
  const keyInput = $<HTMLInputElement>("#tmdb-key");
  api.getTmdbKey().then((k) => { if (k) keyInput.value = k; });
  keyInput.onchange = () => { api.setTmdbKey(keyInput.value.trim()); };

  // 选字幕输入目录
  $<HTMLButtonElement>("#sub-in").onclick = async () => {
    const d = await open({ directory: true });
    if (typeof d === "string") {
      subIn = d; subConfigured = true;
      $("#sub-in-p").textContent = d;
      await api.setSubtitleInputDir(d);
      syncTask(specs[1]);
    }
  };

  // 三个启动按钮：只调 store，进度/结果由 store 驱动 syncFromStore
  $<HTMLButtonElement>("#fetch-posters").onclick = async () => {
    await api.setTmdbKey(keyInput.value.trim());
    startPoster();
  };
  $<HTMLButtonElement>("#sub-run").onclick = () => startSubtitle(subIn || "docs/subtitles");
  $<HTMLButtonElement>("#comic-run").onclick = () => startComic();

  // 订阅 store：任务进度/结果变化时刷新本视图；切走时解绑（不碰 store、不停任务）
  const unsub = subscribe(syncFromStore);
  (el as unknown as { _cleanup?: () => void })._cleanup = () => unsub();

  // 首次回填当前 store 状态（可能已有正在跑或已完成的任务）
  syncFromStore();
  return el;
}
