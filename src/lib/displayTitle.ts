/** 去掉开头的年份前缀（[YYYY]. 或裸 YYYY.），仅供面板展示。
 *  保留片名中间/结尾数字（如「玩命快递3」「铁血战士2010」不变）。
 *  空结果回退原 title，防意外清空。 */
export function displayTitle(title: string): string {
  const t = title.replace(/^\[(\d{4})\]\.?/, "").replace(/^(\d{4})\./, "").trim();
  return t || title;
}
