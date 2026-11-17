/** 是否运行在 Windows（WebView2 的 UA 含 "Windows"；macOS WKWebView 为 "Macintosh"）。 */
const IS_WINDOWS = /windows/i.test(navigator.userAgent);

/**
 * 仅用于展示：把路径分隔符统一成当前系统的格式（Windows 用 \，macOS/其它用 /）。
 * 数据库与后端传参仍保持原样，不要把返回值再存回去。
 */
export function displayPath(p: string): string {
  return IS_WINDOWS ? p.replace(/\//g, "\\") : p.replace(/\\/g, "/");
}

/** 用 / 拼接路径片段，忽略空段并去掉片段首尾多余的分隔符（结果再交给 displayPath 展示）。 */
export function joinPath(...parts: string[]): string {
  return parts
    .filter((s) => s)
    .map((s, i) => (i === 0 ? s.replace(/[\\/]+$/, "") : s.replace(/^[\\/]+|[\\/]+$/g, "")))
    .join("/");
}

const collator = new Intl.Collator("zh-Hans-CN-u-co-pinyin", { numeric: true, sensitivity: "base" });
/** 以 ASCII 字母/数字/符号开头的名称归为「英文」组，排在中文前面。 */
const isLatinFirst = (s: string): boolean => /^[\x00-\x7F]/.test(s.trimStart());

/**
 * 名称排序：英文（含数字开头）在前、中文在后，如 apple → Zeta → 阿拉蕾 → 海贼王。
 * 组内中文按拼音、数字按数值（第2卷 < 第10卷），大小写不敏感。
 */
export const nameCollator = {
  compare(a: string, b: string): number {
    const ga = isLatinFirst(a) ? 0 : 1;
    const gb = isLatinFirst(b) ? 0 : 1;
    return ga !== gb ? ga - gb : collator.compare(a, b);
  },
};
