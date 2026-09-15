import type { PageInfo } from "./ipc";

export type { PageInfo };
export interface Spread { left?: PageInfo; right?: PageInfo; single: boolean; }

/** 是否横向对开图（宽>高）。宽高为 0（解析失败）按竖单页处理。 */
function isWide(p: PageInfo): boolean {
  return p.w > 0 && p.h > 0 && p.w > p.h;
}

/**
 * 页序列 → 对开列表（左→右阅读，封面单页居中）：
 * - 第 1 页（封面）单页居中。
 * - 宽图各自成一个 single 对开（本身即对开）。
 * - 连续竖单页从第 2 页起 (2,3)(4,5)… 两两配对；落单末页 single=false 但只有 left。
 */
export function buildSpreads(pages: PageInfo[]): Spread[] {
  const out: Spread[] = [];
  if (pages.length === 0) return out;
  out.push({ right: pages[0], single: true }); // 封面单页
  let i = 1;
  while (i < pages.length) {
    const cur = pages[i];
    if (isWide(cur)) {
      out.push({ right: cur, single: true });
      i += 1;
      continue;
    }
    const next = pages[i + 1];
    if (next && !isWide(next)) {
      out.push({ left: cur, right: next, single: false });
      i += 2;
    } else {
      out.push({ left: cur, single: false }); // 末页落单，或下一张是宽图
      i += 1;
    }
  }
  return out;
}
