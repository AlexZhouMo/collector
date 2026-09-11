# 卷封面(列表缩略图) + 漫画封面(AniList 拓取) 设计文档

**日期**：2026-09-11
**状态**：已确认

## 背景与动机

上一功能把 zip 首图当作**漫画封面**压缩存 covers/。修正：zip 首图应是**卷封面**（每卷各自的封面，
用作卷列表缩略图，运行时取、不落盘）；**漫画封面**改从网络（AniList）拓取——TMDB 只有影视、无漫画。

## 改动

### 1. scan_comics 不再落盘封面

`scan_comics` 移除 `extract_cover`（Vol_01 首图→to_cover→save_cover）逻辑，扫描时 cover_path 留空
（`None`），等 AniList 拓取填充。签名回退为 `scan_comics(root: &Path) -> Vec<ScannedItem>`（去掉
covers_dir 参数）；`lib.rs` scan_root 的 Comic 分支相应改回不传 covers（app 参数若仅为此加、可保留
无害或去掉——保留以免动 handler，Video/Game 不受影响）。

### 2. 卷列表用 zip 首图缩略图（运行时，不落盘）

新增命令 `comic_volume_cover(zip_path: String) -> AppResult<Option<String>>`：返回该卷 zip 首图的
data URL（复用 `reader::list_pages` 取首图 + `reader::read_entry` + base64，等于旧 comic_cover 逻辑但
按 zip 路径、不压缩不落盘）。`ComicVolumesView` 每张卷卡懒加载显示（替换现在的 book 图标占位；取不到
时回退 book 图标）。

### 3. AniList 漫画封面拓取

新增 `src-tauri/src/poster/anilist.rs`：
- `parse_cover_url(json: &serde_json::Value) -> Option<String>`（**纯函数**）：从 AniList GraphQL 响应
  提取 `data.Media.coverImage.extraLarge`（回退 `large`）。
- `search_cover(name: &str) -> AppResult<Option<String>>`：POST `https://graphql.anilist.co`，
  GraphQL query `query($q:String){ Media(search:$q, type:MANGA){ coverImage{ extraLarge large } } }`，
  variables `{q:name}`，无需 API key；解析走 parse_cover_url。用现有 ureq agent 风格。
- `download(url: &str) -> AppResult<Vec<u8>>`：下载封面字节。

新增命令 `fetch_manga_covers(app) -> poster::FetchReport`（仿 `fetch_posters`，后台 spawn_blocking）：
- 遍历 comic 表**无 cover_path**的漫画（幂等：已有封面跳过）。
- 每部：`search_cover(title)` → 命中则 `download` → `image_proc::to_cover`（居中裁 2:3 → 500×750 →
  JPEG q85，与视频封面一致）→ `image_proc::save_cover(covers, bytes, "manga_")` → `update_cover_path`。
- 未命中/网络错误：记 `FailedItem`（复用 poster 结构，reason="未命中"或错误信息），不中断批量。
- emit `manga-cover-progress` `{done, total, current_title}` 进度。

复用 `poster::FetchReport`/`FailedItem`/`update_cover_path`（需确认可见性，必要时 pub）。

### 4. 前端工具箱按钮

`NormalizeView`（工具箱）新增"漫画封面拓取"`setting-card`：一个 `.btn-primary` 按钮 + 进度条三件套
（复用 `#poster-progress` 同款样式）+ 未命中结果列表（复用 poster-table 或简列表）。调
`api.fetchMangaCovers()`，listen `manga-cover-progress`，完成后可 `scanRoot("comic")` 或直接刷新漫画视图
（首版：完成提示 + 未命中列表，用户回漫画页即见新封面）。

### 5. ipc

- 新增 `fetchMangaCovers: () => invoke<FetchReport>("fetch_manga_covers")`。
- 新增 `comicVolumeCover: (zipPath) => invoke<string|null>("comic_volume_cover", { zipPath })`。
- `ComicVolumesView` 用 comicVolumeCover 显示卷缩略图。

## 数据流

扫描 → comic 入库（cover_path 空）→ 工具箱"漫画封面拓取" → AniList 搜 title → 下载 → 压缩存 covers →
cover_path 入库 → 漫画网格显示封面。卷列表页各卷显示自己 zip 首图缩略（运行时）。

## 错误处理

- AniList 未命中/网络错误：记 FailedItem，继续下一部。
- 已有 cover_path 跳过（幂等）。
- 卷 zip 无首图：卷卡回退 book 图标占位。

## 测试

- `anilist::parse_cover_url`（纯函数）：给定 AniList JSON（含 extraLarge/large/缺失）提取正确 URL、
  缺失返回 None。
- `fetch_manga_covers` 的遍历/跳过已有/失败记录：仿 poster 测试用 mock fetch 闭包（若结构允许），
  或至少覆盖"无 cover 的漫画才处理""失败记入 report"。
- search_cover/download 的真网络部分不单测（与 tmdb 一致）。
- 前端 comicVolumeCover/按钮：tsc + 真机。

## 影响与风险

- scan_comics 去封面落盘（回退上一 Task 的封面逻辑）；新增 anilist 模块、fetch_manga_covers 命令、
  comic_volume_cover 命令；NormalizeView 加按钮；ipc 扩展；ComicVolumesView 用卷缩略图。
- comic.cover_path 语义不变（仍是漫画封面），来源改为 AniList。
- **YAGNI**：不做卷封面落盘、不做 AniList 手动选择候选（首个命中即用）、不做封面缓存过期刷新、
  不动漫画名（不改 title）。
