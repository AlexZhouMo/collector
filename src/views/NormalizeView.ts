import { open } from "@tauri-apps/plugin-dialog";
import { api } from "../lib/ipc";
import { icon } from "../lib/icons";
import { openSubtitleEditor } from "../components/SubtitleEditor";
import { showToast } from "../components/Toast";
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
        <span class="setting-label">输出目录</span>
        <span id="sub-out-p" class="setting-path">加载中…</span>
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
    </div>
    <div class="glass setting-card">
      <div class="setting-card-head"><span class="setting-card-title">系统数据整理</span></div>
      <div class="setting-actions">
        <button class="btn-primary icon-text" id="db-reset-run">${icon("database", 15)}<span class="btn-label">数据库重制</span></button>
        <button class="btn-primary icon-text" id="clean-covers-run">${icon("imageClean", 15)}<span class="btn-label">无效封面清理</span></button>
      </div>
      <div id="maint-result" style="margin-top:10px;font-size:12px;color:var(--text-dim);white-space:pre-line;line-height:1.8"></div>
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
  api.subtitleOutputDir()
    .then((p) => { $("#sub-out-p").textContent = p; })
    .catch(() => {});
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

  // 系统数据整理：直接调后端命令，结果写入 #maint-result（与 store 无关）
  const maintResult = $<HTMLElement>("#maint-result");
  const dbBtn = $<HTMLButtonElement>("#db-reset-run");
  dbBtn.onclick = async () => {
    const label = dbBtn.querySelector<HTMLElement>(".btn-label")!;
    const orig = label.textContent;
    dbBtn.disabled = true; label.textContent = "执行中…";
    try {
      const r = await api.dbReset();
      maintResult.textContent = `数据库重制完成：media ${r.media} 条、comic ${r.comic} 条、game ${r.game} 条，ID 已从 1 重排。`;
    } catch (e) { maintResult.textContent = "数据库重制失败：" + String(e); }
    finally { dbBtn.disabled = false; label.textContent = orig; }
  };
  const coverBtn = $<HTMLButtonElement>("#clean-covers-run");
  coverBtn.onclick = async () => {
    const label = coverBtn.querySelector<HTMLElement>(".btn-label")!;
    const orig = label.textContent;
    coverBtn.disabled = true; label.textContent = "检查中…";
    try {
      const r = await api.cleanCovers();
      let msg = `无效封面清理完成：删除孤立图片 ${r.deleted_orphans} 个。`;
      if (r.missing.length) {
        msg += `\n数据库引用但文件缺失 ${r.missing.length} 个：\n` +
          r.missing.map(m => `· [${m.table}] ${m.title} → ${m.path}`).join("\n");
      } else { msg += "\n未发现缺失文件。"; }
      maintResult.textContent = msg;
    } catch (e) { maintResult.textContent = "无效封面清理失败：" + String(e); }
    finally { coverBtn.disabled = false; label.textContent = orig; }
  };

  // 订阅 store：任务进度/结果变化时刷新本视图；切走时解绑（不碰 store、不停任务）
  const unsub = subscribe(syncFromStore);
  (el as unknown as { _cleanup?: () => void })._cleanup = () => unsub();

  // 字幕告警双击 → 打开行内编辑器（事件委托，结果区是 innerHTML 字符串）
  $("#sub-report").addEventListener("dblclick", (e) => {
    const row = (e.target as HTMLElement).closest<HTMLElement>(".sub-issue");
    if (!row) return;
    if (getState("subtitle").status === "running") { showToast("请等待校准完成"); return; }
    const srcRaw = row.dataset.srcLines ?? "";
    if (!srcRaw) { showToast("该文件无法解码，不能编辑", "error"); return; }
    const srcLines = srcRaw.split(",").map(Number).filter((n) => n > 0);
    if (!srcLines.length) { showToast("无法定位原文行", "error"); return; }
    openSubtitleEditor({
      inDir: subIn || "docs/subtitles",
      file: row.dataset.file ?? "",
      srcLines,
      kind: row.dataset.kind ?? "",
    });
  });

  // 首次回填当前 store 状态（可能已有正在跑或已完成的任务）
  syncFromStore();
  return el;
}
