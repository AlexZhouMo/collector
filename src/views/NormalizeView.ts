import { open } from "@tauri-apps/plugin-dialog";
import { api } from "../lib/ipc";
import { icon } from "../lib/icons";
import { openSubtitleEditor } from "../components/SubtitleEditor";
import { showToast } from "../components/Toast";
import { progressBar } from "../components/ProgressBar";
import {
  getState, subscribe,
  startSubtitle, startComic,
  renderIssueRows,
  type TaskKey,
} from "../lib/normalizeStore";
import type { SubIssue } from "../lib/ipc";

/** 复制到剪贴板：优先 Clipboard API，不可用时退回 execCommand。 */
async function copyText(text: string): Promise<void> {
  try {
    await navigator.clipboard.writeText(text);
    return;
  } catch { /* 退回旧接口 */ }
  const ta = document.createElement("textarea");
  ta.value = text;
  ta.style.cssText = "position:fixed;left:-9999px;top:0";
  document.body.appendChild(ta);
  ta.select();
  const ok = document.execCommand("copy");
  ta.remove();
  if (!ok) throw new Error("剪贴板不可用");
}

// 工具箱：三个长任务的运行态/进度/结果都在 normalizeStore（模块级单例），
// 本视图只负责「渲染 + 从 store 回填」。切菜单销毁本视图不影响任务；切回时
// 重新构造视图并 syncFromStore 续显当前进度/结果。

export function NormalizeView(): HTMLElement {
  const el = document.createElement("div");
  el.className = "view-enter";
  el.innerHTML = `
    <h1 class="page-title">工具箱</h1>
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
        <button class="icon-text" id="sub-out-open">${icon("folder", 15)}<span class="btn-label">打开</span></button>
      </div>
      <div class="setting-actions">
        <button class="icon-text" id="sub-run" disabled>${icon("play", 15)}<span class="btn-label">开始校准</span></button>
      </div>
      ${progressBar("sub-progress", "margin-top:22px")}
      <div id="sub-report" style="margin-top:10px"></div>
    </div>
    <div class="glass setting-card">
      <div class="setting-card-head"><span class="setting-card-title">漫画自动归档</span></div>
      <div class="setting-row">
        <span class="setting-label">漫画目录</span>
        <span id="comic-dir-p" class="setting-path">未配置</span>
      </div>
      <div class="setting-actions">
        <button class="icon-text" id="comic-run" disabled>${icon("archive", 15)}<span class="btn-label">开始归档</span></button>
      </div>
      ${progressBar("comic-progress", "margin-top:22px")}
      <div id="comic-report" style="margin-top:10px"></div>
    </div>
    <div class="glass setting-card">
      <div class="setting-card-head"><span class="setting-card-title">系统数据整理</span></div>
      <div class="setting-actions">
        <button class="icon-text" id="db-reset-run">${icon("database", 15)}<span class="btn-label">数据库重置</span></button>
        <button class="icon-text" id="clean-covers-run">${icon("imageClean", 15)}<span class="btn-label">无效封面清理</span></button>
      </div>
    </div>`;

  // 配置就绪与否（与任务运行态一起决定 sub/comic 按钮是否可点）
  let subIn = "";
  let subConfigured = false;
  let comicConfigured = false;

  const $ = <T extends HTMLElement>(sel: string) => el.querySelector<T>(sel)!;

  // 两个任务（字幕/漫画）：按钮/进度容器/进度条/文字/结果区，及 running 时的按钮文案
  const specs: { key: TaskKey; btn: string; prog: string; bar: string; text: string; result: string; idle: string; busy: string; enabled: () => boolean }[] = [
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

  // 配置读取（与 store 无关）：字幕输入目录、漫画根目录
  api.getSubtitleInputDir().then((d) => {
    if (d) {
      subIn = d; subConfigured = true;
      $("#sub-in-p").textContent = d;
      syncTask(specs[0]);
    }
  });
  api.subtitleOutputDir()
    .then((p) => { $("#sub-out-p").textContent = p; })
    .catch((e) => { $("#sub-out-p").textContent = "无法获取：" + e; });
  $<HTMLButtonElement>("#sub-out-open").onclick = () =>
    api.openSubtitleOutputDir().catch((e) => showToast("打开失败：" + e, "error"));
  api.getRoot("comic").then((d) => {
    if (d) {
      comicConfigured = true;
      $("#comic-dir-p").textContent = d;
      syncTask(specs[1]);
    } else {
      $("#comic-dir-p").textContent = "请先在设置中配置漫画根目录";
    }
  });

  // 选字幕输入目录
  $<HTMLButtonElement>("#sub-in").onclick = async () => {
    const d = await open({ directory: true });
    if (typeof d === "string") {
      subIn = d; subConfigured = true;
      $("#sub-in-p").textContent = d;
      await api.setSubtitleInputDir(d);
      syncTask(specs[0]);
    }
  };

  // 启动按钮：只调 store，进度/结果由 store 驱动 syncFromStore
  $<HTMLButtonElement>("#sub-run").onclick = () => startSubtitle(subIn || "docs/subtitles");
  $<HTMLButtonElement>("#comic-run").onclick = () => startComic();

  // 系统数据整理：直接调后端命令，结果用与设置页导入/导出相同的 toast 提示（与 store 无关）
  const dbBtn = $<HTMLButtonElement>("#db-reset-run");
  dbBtn.onclick = async () => {
    const label = dbBtn.querySelector<HTMLElement>(".btn-label")!;
    const orig = label.textContent;
    dbBtn.disabled = true; label.textContent = "执行中…";
    try {
      const r = await api.dbReset();
      showToast(`重置完成：影视 ${r.media} 条、漫画 ${r.comic} 条、游戏 ${r.game} 条。`);
    } catch (e) { showToast("数据库重置失败：" + String(e), "error"); }
    finally { dbBtn.disabled = false; label.textContent = orig; }
  };
  const coverBtn = $<HTMLButtonElement>("#clean-covers-run");
  coverBtn.onclick = async () => {
    const label = coverBtn.querySelector<HTMLElement>(".btn-label")!;
    const orig = label.textContent;
    coverBtn.disabled = true; label.textContent = "检查中…";
    try {
      const r = await api.cleanCovers();
      showToast(`删除孤立图片 ${r.deleted_orphans} 个；发现缺失文件 ${r.missing.length} 个。`);
    } catch (e) { showToast("无效封面清理失败：" + String(e), "error"); }
    finally { coverBtn.disabled = false; label.textContent = orig; }
  };

  // 订阅 store：任务进度/结果变化时刷新本视图；切走时解绑（不碰 store、不停任务）
  const unsub = subscribe(syncFromStore);
  (el as unknown as { _cleanup?: () => void })._cleanup = () => unsub();

  // 编辑器保存后：只重绘该文件那一组提示（保持展开），已修好的记录随重校结果消失
  const patchFileIssues = (file: string, issues: SubIssue[]) => {
    const report = $("#sub-report");
    const details = Array.from(report.querySelectorAll<HTMLElement>("details.sub-file"))
      .find((d) => d.dataset.file === file);
    if (details) {
      if (issues.length) {
        details.querySelector<HTMLElement>(".sub-issues")!.innerHTML = renderIssueRows(file, issues);
        details.querySelector<HTMLElement>(".sub-badge")!.textContent = String(issues.length);
      } else {
        details.remove();
      }
    }
    const total = report.querySelectorAll(".sub-issue").length;
    const countBadge = report.querySelector<HTMLElement>(".sub-count-badge");
    if (countBadge) {
      countBadge.textContent = total ? `${total} 条提示` : "无质检问题";
      countBadge.classList.toggle("has", total > 0);
      countBadge.classList.toggle("none", total === 0);
    }
  };

  // 字幕告警双击 → 打开行内编辑器（事件委托，结果区是 innerHTML 字符串）
  $("#sub-report").addEventListener("dblclick", (e) => {
    if ((e.target as HTMLElement).closest(".sub-copy-btn")) return;
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
      onSaved: patchFileIssues,
    });
  });

  // 行尾复制按钮：把对应的标记字符串复制到剪贴板（不改原文，供人工修改）
  $("#sub-report").addEventListener("click", async (e) => {
    const btn = (e.target as HTMLElement).closest<HTMLButtonElement>(".sub-copy-btn");
    if (!btn) return;
    e.stopPropagation();
    const text = btn.dataset.copy ?? "";
    try {
      await copyText(text);
      showToast(`已复制「${text}」`);
    } catch (err) {
      showToast("复制失败：" + String(err), "error");
    }
  });

  // 首次回填当前 store 状态（可能已有正在跑或已完成的任务）
  syncFromStore();
  return el;
}
