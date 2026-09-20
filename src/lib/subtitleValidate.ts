// 编辑器保存前的即时预校验（与后端 subtitle_check 语义一致）。
// 权威校验仍在后端；此处仅为即时反馈。

export interface RowInput { lineNo: number; start: string; end: string; text: string; }
export interface RowProblem { lineNo: number; field: "start" | "end" | "text"; msg: string; }

const TIME_RE = /^\d:\d{2}:\d{2}\.\d{2}$/;

/** "H:MM:SS.CS" → 厘秒；非法返回 null。 */
export function parseTimeCs(t: string): number | null {
  if (!TIME_RE.test(t)) return null;
  const [hms, cs] = t.split(".");
  const [h, m, s] = hms.split(":").map(Number);
  return ((h * 60 + m) * 60 + s) * 100 + Number(cs);
}

/** 正文异常：含 ",." 或连续两空格；或异常 ".,"（点号前非字母）。 */
export function textProblem(text: string): string | null {
  if (text.includes(",.")) return '含可疑标点 ",."';
  if (text.includes("  ")) return "含连续两个空格";
  for (let i = 0; i < text.length; i++) {
    if (text[i] === "." && text[i + 1] === ",") {
      const prev = text[i - 1];
      if (!prev || !/[A-Za-z]/.test(prev)) return '含可疑标点 ".,"';
    }
  }
  return null;
}

/** 时间轴交叉：排序后每条与其后 3 条比区间重叠；起止完全相同不算交叉。 */
function crossProblems(rows: RowInput[]): RowProblem[] {
  const withCs = rows
    .map((r) => ({ r, s: parseTimeCs(r.start), e: parseTimeCs(r.end) }))
    .filter((x) => x.s !== null && x.e !== null) as { r: RowInput; s: number; e: number }[];
  withCs.sort((a, b) => a.s - b.s);
  const out: RowProblem[] = [];
  for (let a = 0; a < withCs.length; a++) {
    for (let b = a + 1; b < Math.min(a + 4, withCs.length); b++) {
      const A = withCs[a], B = withCs[b];
      const identical = A.s === B.s && A.e === B.e;
      if (!identical && A.s < B.e && B.s < A.e) {
        out.push({ lineNo: B.r.lineNo, field: "start", msg: "时间轴与相邻行交叉" });
      }
    }
  }
  return out;
}

/** 全部预校验：返回问题列表（空 = 无问题）。 */
export function validateRows(rows: RowInput[]): RowProblem[] {
  const problems: RowProblem[] = [];
  for (const r of rows) {
    if (parseTimeCs(r.start) === null) problems.push({ lineNo: r.lineNo, field: "start", msg: "时间格式非法" });
    if (parseTimeCs(r.end) === null) problems.push({ lineNo: r.lineNo, field: "end", msg: "时间格式非法" });
    const cs = parseTimeCs(r.start), ce = parseTimeCs(r.end);
    if (cs !== null && ce !== null && cs >= ce) problems.push({ lineNo: r.lineNo, field: "end", msg: "结束不晚于开始" });
    const tp = textProblem(r.text);
    if (tp) problems.push({ lineNo: r.lineNo, field: "text", msg: tp });
  }
  problems.push(...crossProblems(rows));
  return problems;
}
