export interface MenuItem {
  label: string;
  danger?: boolean;
  onClick: () => void;
}

/// 在屏幕 (x, y) 处弹出右键菜单。点菜单项执行 onClick 并关闭；
/// 点菜单外部或按 Esc 关闭。同一时刻只保留一个菜单。
export function showContextMenu(x: number, y: number, items: MenuItem[]): void {
  // 先移除已有菜单
  document.querySelectorAll(".context-menu").forEach((m) => m.remove());

  const menu = document.createElement("div");
  menu.className = "context-menu glass";

  // close/onOutside/onKey 需在 forEach 中被引用，故定义在 forEach 之前，避免 TDZ。
  const close = () => {
    menu.remove();
    document.removeEventListener("mousedown", onOutside, true);
    document.removeEventListener("keydown", onKey, true);
  };
  const onOutside = (e: MouseEvent) => {
    if (!menu.contains(e.target as Node)) close();
  };
  const onKey = (e: KeyboardEvent) => {
    if (e.key === "Escape") close();
  };

  items.forEach((it) => {
    const el = document.createElement("div");
    el.className = "context-item" + (it.danger ? " danger" : "");
    el.textContent = it.label;
    el.onclick = (e) => {
      e.stopPropagation();
      close();
      it.onClick();
    };
    menu.appendChild(el);
  });
  document.body.appendChild(menu);

  // 定位：避免超出视口右/下边缘
  const rect = menu.getBoundingClientRect();
  const px = Math.min(x, window.innerWidth - rect.width - 8);
  const py = Math.min(y, window.innerHeight - rect.height - 8);
  menu.style.left = `${Math.max(0, px)}px`;
  menu.style.top = `${Math.max(0, py)}px`;

  // 下一帧再挂监听，避免触发本次右键的 mousedown 立即关闭
  setTimeout(() => {
    document.addEventListener("mousedown", onOutside, true);
    document.addEventListener("keydown", onKey, true);
  }, 0);
}
