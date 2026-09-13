/**
 * 让一个承载名字的元素支持内联重命名。
 * el: 名字元素（点击后就地替换为 input，编辑结束再恢复）；
 * current: 当前名字；
 * onCommit(newName): 确认保存回调，返回 Promise——resolve 表示保存成功（调用方通常会重建 UI），
 *   reject 表示保存失败（组件保持 input 在编辑态并重新聚焦）。
 * 交互：点击进入编辑（input 值=current、全选、聚焦）；回车或 blur 提交；Esc 取消恢复原名。
 * newName 去空后为空或等于 current → 视为取消（不调 onCommit）。
 */
export function attachInlineRename(
  el: HTMLElement,
  current: string,
  onCommit: (newName: string) => Promise<void>
): void {
  el.onclick = (e) => {
    e.stopPropagation(); // 防止触发文件夹进入等外层点击
    enterEdit();
  };

  const enterEdit = () => {
    const input = document.createElement("input");
    input.className = "fv-name-edit";
    input.value = current;
    const parent = el.parentElement;
    if (!parent) return;
    el.style.display = "none";
    parent.insertBefore(input, el.nextSibling);
    input.focus();
    input.select();

    let committing = false;
    let cancelled = false;

    const restore = () => {
      input.remove();
      el.style.display = "";
    };

    const commit = async () => {
      if (committing || cancelled) return;
      const name = input.value.trim();
      if (!name || name === current) { restore(); return; } // 空/无改动 → 取消
      committing = true;
      try {
        await onCommit(name);
        // 成功：调用方会重建 UI；此处也移除 input 兜底
        restore();
      } catch {
        // 失败：留在编辑态，重新聚焦让用户改
        committing = false;
        input.focus();
        input.select();
      }
    };

    input.addEventListener("keydown", (ev) => {
      if (ev.key === "Enter") { ev.preventDefault(); commit(); }
      else if (ev.key === "Escape") { ev.preventDefault(); cancelled = true; restore(); }
      ev.stopPropagation(); // 不冒泡给全局 Esc（如播放器）
    });
    input.addEventListener("blur", () => { if (!cancelled) commit(); });
  };
}
