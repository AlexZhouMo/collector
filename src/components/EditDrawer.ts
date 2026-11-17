import { api } from "../lib/ipc";
import type { MediaItem } from "../lib/ipc";
import { open } from "@tauri-apps/plugin-dialog";
import { convertFileSrc } from "@tauri-apps/api/core";
import { openCoverCropper } from "./CoverCropper";
import { esc } from "../lib/escape";
import { icon } from "../lib/icons";
import { isValidCategoryPath } from "../lib/pathValidate";
import { displayPath, joinPath } from "../lib/displayPath";
import { showToast } from "./Toast";

/// 无封面时的占位图：2:3 内联 SVG（不依赖外部文件），半透明灰在深浅主题下都可见，
/// 底色交给 .drawer-cover-preview 的 background。
const COVER_PLACEHOLDER = "data:image/svg+xml;charset=utf-8," + encodeURIComponent(
  `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 200 300">
    <g fill="none" stroke="rgba(150,150,150,.75)" stroke-width="6" stroke-linejoin="round" stroke-linecap="round">
      <rect x="55" y="100" width="90" height="70" rx="8"/>
      <circle cx="80" cy="122" r="8"/>
      <path d="M60 162l28-26 18 16 12-10 22 20"/>
    </g>
    <text x="100" y="205" text-anchor="middle" font-size="18" fill="rgba(150,150,150,.9)"
      font-family="system-ui,-apple-system,'Microsoft YaHei',sans-serif">暂无海报</text>
  </svg>`);

/// 抽屉右上角关闭按钮的 ✕ 图形
const CLOSE_SVG = `<svg viewBox="0 0 16 16" width="14" height="14" aria-hidden="true"><path d="M4 4l8 8M12 4l-8 8" stroke="currentColor" stroke-width="1.8" stroke-linecap="round"/></svg>`;

/// 打开右侧滑入抽屉表单，用于新增(item=null)或编辑(item 有值)视频条目。
/// 保存成功后关闭抽屉并回调 onSaved()。取消/遮罩点击/Esc 关闭不保存。
/// defaultCategory：新增时落入的分类（分类与分类路径不再在表单里编辑，
/// 编辑保留原值，新增用当前分类作为分类与分类路径）。
export function openEditDrawer(item: MediaItem | null, onSaved: () => void, defaultCategory = "电影", kind: "video" | "comic" | "game" = "video", createPath?: string): void {
  // 局部维护的可变字段（文件选择后更新）
  let coverPath = item?.cover_path ?? "";

  // 分类与分类路径不在表单里编辑：编辑保留原值，新增落入当前分类根。
  const category = item?.category ?? defaultCategory;
  let categoryPath = item?.category_path ?? createPath ?? "";

  const overlay = document.createElement("div");
  overlay.className = "drawer-overlay";

  const drawer = document.createElement("div");
  drawer.className = "edit-drawer glass";

  // 漫画编辑时不提供封面上传（漫画封面只在「卷」一级显示），只改标题和简介。
  // 封面区仅隐藏不移除：下方逻辑照常运行，coverPath 保持原值，保存时不会被清空。
  const hideCover = kind === "comic" && !!item;

  drawer.innerHTML = `
    <div class="drawer-head">
      <span class="drawer-head-ic">${icon(kind === "comic" ? "book" : kind === "game" ? "gamepad" : "film", 18)}</span>
      <h3 class="drawer-head-title">${kind === "comic" ? (item ? "编辑漫画" : "新增漫画") : kind === "game" ? (item ? "编辑游戏" : "新增游戏") : (item ? "编辑视频" : "新增视频")}</h3>
      <button type="button" class="drawer-close" data-b="close" title="关闭（Esc）">${CLOSE_SVG}</button>
    </div>
    <div class="drawer-field">
      <label>标题</label>
      <input type="text" data-f="title" value="${esc(item?.title ?? "")}" placeholder="输入标题" />
    </div>
    ${!item ? `<div class="drawer-field">
      <label>目录路径</label>
      <input type="text" data-f="path" value="${esc(createPath ?? "")}" placeholder="分类内目录路径，如 犯罪题材/教父（留空=分类根）" />
      <div data-d="path-hint"></div>
    </div>` : ""}
    ${!item ? "" : `<div class="drawer-field">
      <label>${kind === "comic" ? "漫画路径" : kind === "game" ? "游戏路径" : "视频路径"}</label>
      <div class="drawer-path"><span data-d="item-path">${kind === "video" ? esc(item.video_path ? displayPath(item.video_path) : "（未定位到视频文件）") : "加载中…"}</span></div>
    </div>`}
    <div class="drawer-field"${hideCover ? ' style="display:none"' : ""}>
      <label>封面</label>
      <div class="cover-box" data-d="coverbox">
        <img class="drawer-cover-preview" data-d="cover" alt="" />
        <button type="button" class="cover-del" data-b="cover-del" title="删除封面">${icon("trash", 15)}</button>
      </div>
      <div class="cover-actions">
        <button type="button" class="icon-text" data-b="cover">${icon("upload", 14)}<span class="btn-label">选择图片</span></button>
        ${kind === "video" ? `<button type="button" class="icon-text" data-b="cover-auto" title="按标题从 TMDB 抓取海报">${icon("refresh", 14)}<span class="btn-label">自动抓取</span></button>` : ""}
      </div>
    </div>
    <div class="drawer-field">
      <label>简介</label>
      <textarea data-f="description" rows="5" placeholder="写点简介…">${esc(item?.description ?? "")}</textarea>
    </div>
    <div class="drawer-actions">
      <button type="button" data-b="cancel">取消</button>
      <button type="button" class="btn-primary" data-b="save">保存</button>
    </div>
  `;

  const q = <T extends HTMLElement>(sel: string): T => drawer.querySelector(sel) as T;

  // 漫画/游戏路径：由根目录 + 分类路径（漫画再加标题）推导，仅展示用（分隔符跟随当前系统）
  if (item && kind !== "video") {
    const pathEl = q<HTMLElement>('[data-d="item-path"]');
    api.getRoot(kind).then((root) => {
      pathEl.textContent = root
        ? displayPath(kind === "comic" ? joinPath(root, item.category_path, item.title) : joinPath(root, item.category_path))
        : `（未设置${kind === "comic" ? "漫画" : "游戏"}目录）`;
    }).catch(() => { pathEl.textContent = "（读取失败）"; });
  }
  const coverImg = q<HTMLImageElement>('[data-d="cover"]');
  const coverBox = q<HTMLDivElement>('[data-d="coverbox"]');

  // 记录进入编辑时的原封面路径，保存时据此删旧文件
  const originalCover = item?.cover_path ?? "";

  // 无封面（或封面文件读不到）时显示占位图，并隐藏删除图标（占位图不可删）
  const showPlaceholder = () => {
    coverImg.src = COVER_PLACEHOLDER;
    coverBox.classList.add("is-placeholder");
  };
  coverImg.onerror = () => { if (coverImg.src !== COVER_PLACEHOLDER) showPlaceholder(); };
  const refreshCover = () => {
    if (coverPath) {
      coverBox.classList.remove("is-placeholder");
      coverImg.src = convertFileSrc(coverPath);
    } else {
      showPlaceholder();
    }
  };
  // 删除封面：仅清本地预览，保存时才真正删库+删文件
  q<HTMLButtonElement>('[data-b="cover-del"]').onclick = () => { coverPath = ""; refreshCover(); };
  refreshCover();

  // 新增模式：目录路径框实时校验，非法则禁用保存（disabled 触发全局 button:disabled 样式）
  if (!item) {
    const pathInput = q<HTMLInputElement>('[data-f="path"]');
    const pathHint = q<HTMLElement>('[data-d="path-hint"]');
    const saveBtn = q<HTMLButtonElement>('[data-b="save"]');
    const validatePath = () => {
      const v = pathInput.value;
      if (isValidCategoryPath(v)) {
        categoryPath = v;
        pathHint.textContent = "";
        saveBtn.disabled = false;
      } else {
        pathHint.textContent = "路径含非法字符或格式错误";
        saveBtn.disabled = true;
      }
    };
    pathInput.oninput = validatePath;
    validatePath(); // 初始校验（默认值一般合法）
  }

  // 关闭机制
  const onKey = (e: KeyboardEvent) => { if (e.key === "Escape") close(); };
  const close = () => {
    overlay.remove();
    drawer.remove();
    document.removeEventListener("keydown", onKey, true);
  };
  overlay.onclick = () => close();
  q<HTMLButtonElement>('[data-b="close"]').onclick = () => close();
  document.addEventListener("keydown", onKey, true);


  // 封面选择 → 裁剪弹窗 → 后端生成标准海报 → 预览
  q<HTMLButtonElement>('[data-b="cover"]').onclick = async () => {
    const f = await open({ multiple: false, filters: [{ name: "图片", extensions: ["jpg", "jpeg", "png", "webp"] }] });
    if (typeof f === "string") {
      openCoverCropper(f, (p) => { coverPath = p; refreshCover(); }, kind);
    }
  };

  // 自动抓取（仅影视）：按当前标题/目录联 TMDB 抓海报，只更新预览，保存时才落库
  const autoBtn = drawer.querySelector<HTMLButtonElement>('[data-b="cover-auto"]');
  if (autoBtn) {
    const autoLabel = autoBtn.querySelector<HTMLElement>(".btn-label")!;
    autoBtn.onclick = async () => {
      const title = q<HTMLInputElement>('[data-f="title"]').value.trim();
      if (!title) { showToast("请先填写标题", "error"); return; }
      autoBtn.disabled = true;
      autoLabel.textContent = "抓取中…";
      try {
        coverPath = await api.fetchPosterFor(category, categoryPath, title);
        refreshCover();
        showToast("已抓取海报，点「保存」后生效");
      } catch (e) {
        showToast(String(e), "error");
      } finally {
        autoBtn.disabled = false;
        autoLabel.textContent = "自动抓取";
      }
    };
  }

  // 保存
  q<HTMLButtonElement>('[data-b="save"]').onclick = async () => {
    const title = q<HTMLInputElement>('[data-f="title"]').value.trim();
    const desc = q<HTMLTextAreaElement>('[data-f="description"]').value.trim();
    try {
      if (kind === "comic") {
        // 漫画：编辑现有条目更新，新增则落入当前分类路径
        if (item?.id) {
          await api.comicUpdate(item.id, categoryPath, title,
            coverPath || null, desc || null);
        } else {
          await api.mediaCreate("comic", "", categoryPath, title,
            coverPath || null, desc || null);
        }
      } else if (kind === "game") {
        // 游戏：编辑现有条目更新，新增则落入当前分类路径
        if (item?.id) {
          await api.gameUpdate(item.id, categoryPath, title,
            coverPath || null, desc || null);
        } else {
          await api.mediaCreate("game", "", categoryPath, title,
            coverPath || null, desc || null);
        }
      } else if (item?.id) {
        await api.mediaUpdate(item.id, category, categoryPath, title,
          coverPath || null, desc || null);
      } else {
        await api.mediaCreate("video", category, categoryPath, title,
          coverPath || null, desc || null);
      }
      // 原封面被删或被换新图 → 删除旧磁盘文件（失败忽略，不阻断保存）
      if (originalCover && originalCover !== coverPath) {
        try { await api.deleteCoverFile(originalCover); } catch { /* 忽略 */ }
      }
      close();
      onSaved();
    } catch (err) {
      alert("保存失败：" + String(err));
    }
  };

  // 取消
  q<HTMLButtonElement>('[data-b="cancel"]').onclick = () => close();

  document.body.appendChild(overlay);
  document.body.appendChild(drawer);
}
