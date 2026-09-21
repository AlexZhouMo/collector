import { api } from "../lib/ipc";
import type { LineEdit } from "../lib/ipc";
import { esc } from "../lib/escape";
import { showToast } from "./Toast";
import { validateRows, type RowInput, type RowProblem } from "../lib/subtitleValidate";
import { kindColor, updateFileIssues } from "../lib/normalizeStore";

interface OpenOpts {
  inDir: string;
  file: string;
  srcLines: number[];
  kind: string;
}

/** 双击告警打开：读原文目标行±8 行，可编辑表格，保存前预校验+二次确认，写回后重校刷新列表。 */
export async function openSubtitleEditor(opts: OpenOpts): Promise<void> {
  const { inDir, file, srcLines, kind } = opts;
  let lines;
  try {
    lines = await api.readSubtitleContext(inDir, file, srcLines, 8);
  } catch (e) {
    showToast(String(e), "error");
    return;
  }

  const overlay = document.createElement("div");
  overlay.className = "modal-overlay";
  const dialog = document.createElement("div");
  dialog.className = "sub-editor glass";
  overlay.appendChild(dialog);

  const close = () => {
    overlay.remove();
    document.removeEventListener("keydown", onKey, true);
  };
  const onKey = (e: KeyboardEvent) => { if (e.key === "Escape") close(); };
  document.addEventListener("keydown", onKey, true);

  const rowsHtml = lines.map((l) => `
    <tr data-line="${l.lineNo}"${l.isTarget ? ' class="target"' : ""}>
      <td class="se-no">${l.lineNo}</td>
      <td><input class="se-start" value="${esc(l.start)}" /></td>
      <td><input class="se-end" value="${esc(l.end)}" /></td>
      <td><input class="se-text" value="${esc(l.text)}" /></td>
    </tr>`).join("");

  dialog.innerHTML = `
    <div class="se-head">
      <span class="se-file" title="${esc(file)}">${esc(file)}</span>
      <span class="se-kind" style="--k:${kindColor(kind)}">${esc(kind)}</span>
    </div>
    <div class="se-problems" style="display:none"></div>
    <div class="se-table-wrap">
      <table class="se-table">
        <colgroup>
          <col class="se-c-no" /><col class="se-c-time" /><col class="se-c-time" /><col class="se-c-body" />
        </colgroup>
        <thead><tr><th>行号</th><th>开始</th><th>结束</th><th>正文</th></tr></thead>
        <tbody>${rowsHtml}</tbody>
      </table>
    </div>
    <div class="se-actions">
      <button class="se-cancel">取消</button>
      <button class="btn-primary se-save">保存</button>
    </div>`;

  document.body.appendChild(overlay);
  overlay.addEventListener("mousedown", (e) => { if (e.target === overlay) close(); });
  dialog.querySelector<HTMLButtonElement>(".se-cancel")!.onclick = close;

  // 目标行滚入视野
  const targetTr = dialog.querySelector<HTMLElement>("tr.target");
  targetTr?.scrollIntoView({ block: "center" });

  const collectRows = (): RowInput[] =>
    Array.from(dialog.querySelectorAll<HTMLElement>("tbody tr")).map((tr) => ({
      lineNo: Number(tr.dataset.line),
      start: tr.querySelector<HTMLInputElement>(".se-start")!.value.trim(),
      end: tr.querySelector<HTMLInputElement>(".se-end")!.value.trim(),
      text: tr.querySelector<HTMLInputElement>(".se-text")!.value,
    }));

  const paintProblems = (problems: RowProblem[]) => {
    dialog.querySelectorAll(".se-start,.se-end,.se-text").forEach((el) => el.classList.remove("bad"));
    const box = dialog.querySelector<HTMLElement>(".se-problems")!;
    if (!problems.length) { box.style.display = "none"; box.innerHTML = ""; return; }
    for (const p of problems) {
      const tr = dialog.querySelector<HTMLElement>(`tr[data-line="${p.lineNo}"]`);
      tr?.querySelector(`.se-${p.field}`)?.classList.add("bad");
    }
    box.style.display = "block";
    box.innerHTML = problems.map((p) => `<div>L${p.lineNo}：${esc(p.msg)}</div>`).join("");
  };

  const doSave = async () => {
    const rows = collectRows();
    const edits: LineEdit[] = rows.map((r) => ({ lineNo: r.lineNo, start: r.start, end: r.end, text: r.text }));
    try {
      const issues = await api.saveSubtitleEdits(inDir, file, edits);
      updateFileIssues(file, issues);
      close();
      showToast(`已保存，该文件剩余 ${issues.length} 条提示`);
    } catch (e) {
      showToast(String(e), "error");
    }
  };

  dialog.querySelector<HTMLButtonElement>(".se-save")!.onclick = () => {
    const problems = validateRows(collectRows());
    paintProblems(problems);
    if (problems.length && !confirm(`仍有 ${problems.length} 处问题，确定保存？`)) return;
    void doSave();
  };
}
