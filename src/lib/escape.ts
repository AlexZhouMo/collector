/** 转义 HTML 特殊字符，用于安全地插入 innerHTML 文本内容。 */
export function esc(s: string): string {
  return s.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;").replace(/"/g, "&quot;");
}
