/** 分类内目录路径合法性校验（基本非法字符）。
 * 禁非法字符 \ : * ? " < > | 及控制字符；不以 / 开头或结尾；不含空段 //；
 * 任一段不为 . 或 ..、不为纯空白；允许中文、空格、多层 a/b/c。空串合法（分类根）。 */
export function isValidCategoryPath(path: string): boolean {
  if (path === "") return true;
  if (path.startsWith("/") || path.endsWith("/")) return false;
  if (/[\\:*?"<>|]/.test(path)) return false;
  // eslint-disable-next-line no-control-regex
  if (/[\x00-\x1f]/.test(path)) return false;
  for (const s of path.split("/")) {
    if (s === "" || s === "." || s === ".." || s.trim() === "") return false;
  }
  return true;
}
