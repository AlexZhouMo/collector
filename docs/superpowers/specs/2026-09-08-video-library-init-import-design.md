# 视频库初始化导入（demo/subtitles 种子结构）设计文档

- 日期：2026-09-08
- 状态：设计待复审
- 范围：子项目 1/2（本项目：初始化导入；后续项目：面板内视频 CRUD，另行设计）

## Context

当前视频库靠"扫描用户配置的根目录找 .mkv"填充，但用户尚无 mkv，希望先用 `demo/subtitles` 里现成的分类+剧目结构把媒体库**初始化**出来：清空数据库，从 ID=1 顺序导入每个视频条目。产品方向同时转变：**设置页配置视频路径只保存、不再自动触发扫描**；后续用户在内容面板内直接管理（增删改）视频——面板 CRUD 是独立子项目，本次只做初始化导入这一地基。

`demo/subtitles` 结构（已探查）：顶层 `电影/ 动漫/ 剧集/ 限制级/`，其下多级 `分类/子分类/剧名/*.ass`（如 `电影/奇幻/哈利波特/[2007].哈利波特与凤凰社.ass`、`剧集/日剧/怨屋本铺/E07.xxx.ass`）。**只有 .ass 字幕、无 .mkv**。

## 已确认的设计决策

1. **条目依据 = .ass 文件**：每个 .ass 是一个视频条目。`title` = 字幕文件名去扩展名（如 `[2007].哈利波特与凤凰社`）；`subtitle_path` = 该 .ass 绝对路径；`path`（视频文件）暂为空串（后续放同名 .mkv）。
2. **分类名「电视剧」→「剧集」**：应用视频三分类改为 **电影 / 动漫 / 剧集**（匹配 demo 目录名）。涉及 `scan_videos_all` 的分类映射、前端 SettingsView 的 VIDEO_CATS 显示名、VideoView 的分类 tab。
3. **只导入电影/动漫/剧集三类**，`限制级` 目录不导入。
4. **清库 + ID 从 1 顺序导入**：清空 media_item（级联清 watch_state/game_state），**重置 sqlite_sequence**（`DELETE FROM sqlite_sequence WHERE name='media_item'`）使自增 id 从 1；导入前**按 category_path + title 排序**，再顺序插入，ID 稳定可预期。
5. **展示图用现有 cover_path 列**（不新增列）：导入时若剧名目录内有展示图（poster.jpg 或与视频同名的 .jpg）则填 cover_path，否则空。
6. **设置页改为只保存不扫描**：配置视频/漫画/游戏路径只 `set_root` 保存，移除"扫描"自动触发（扫描按钮行为调整/移除）。视频库靠本次初始化导入 + 后续面板管理，不再依赖设置页扫描。

## 架构

在现有 scanner/library 上做针对性改动，新增一个"初始化导入"入口，不引入新子系统。

### 后端 `src-tauri/src/library/scanner.rs`
- **`scan_videos` 改为以 .ass 为条目**（或新增 `scan_videos_subs`）：当前找 `.mkv`，改为找 `.ass`。对每个 .ass：
  - `title` = stem（去 .ass）
  - `subtitle_path` = 该 .ass 绝对路径
  - `path` = ""（视频文件待定）
  - `category` = 传入分类名（电影/动漫/剧集）
  - `category_path` = category + 目录内相对路径（保留 子分类/剧名 层级）
  - `cover_path` = 同目录 poster.jpg 或同名 .jpg（存在才填）
  - `description` = 同目录 info.txt（存在才填）
- 注：文档注释里"电视剧"改"剧集"。

### 后端 `src-tauri/src/library/mod.rs`
- **`replace_items` 增加序列重置**：DELETE media_item 后加 `DELETE FROM sqlite_sequence WHERE name='media_item'`（在同一事务内），使重扫/导入后 id 从 1。
- **导入前排序**：`replace_items`（或调用方）对 items 按 `(category_path, title)` 排序后再插入，保证 ID 顺序稳定。

### 后端初始化导入入口 `src-tauri/src/lib.rs`
- 新增 command `init_from_demo`（或复用改造的扫描）：以 `demo/subtitles` 为根，对电影/动漫/剧集三目录分别 `scan_videos_subs(root/分类, 分类名)`，合并、排序，`replace_items(Video, all)`（清库+重置序列+顺序插入）。返回导入条目数。
- **触发方式**：设置页视频区加一个「初始化示例库」按钮调 `init_from_demo`（手动触发、幂等——每次清库重导）。demo 路径定位：`demo/subtitles` 相对项目根（开发期）；command 内用一个已知/传入的绝对路径解析。
- demo 目录定位：相对项目根的 `demo/subtitles`（开发期）——由前端把该路径传给 command，或 command 内解析项目根。（初始化是一次性/可重复的手动操作。）

### 后端 `scan_videos_all` 分类映射
- `("video_tv", "电视剧")` → `("video_tv", "剧集")`（分类名改剧集）。

### 前端
- `SettingsView.ts`：VIDEO_CATS 的 `["video_tv", "电视剧"]` → `["video_tv", "剧集"]`；**扫描按钮改为只保存不扫描**（移除 scanVideos/scanRoot 自动调用，或保留"保存"语义）。
- `VideoView.ts`：分类 tab `["电影","动漫","电视剧"]` → `["电影","动漫","剧集"]`。

## 关键文件
- `src-tauri/src/library/scanner.rs`（.ass 为条目、剧集）
- `src-tauri/src/library/mod.rs`（replace_items 重置序列 + 排序）
- `src-tauri/src/lib.rs`（init_from_demo 入口、分类映射改剧集）
- `src/views/SettingsView.ts`（剧集、只保存不扫描）
- `src/views/VideoView.ts`（分类 tab 剧集）

## 明确不做（YAGNI，属后续子项目或不需要）
- 不做面板内视频增删改 CRUD（独立子项目 2）
- 不导入限制级目录
- 不新增数据库列（用现有 cover_path）
- 不处理 .mkv 实际播放（本次导入的条目 path 为空，播放待后续放入 mkv）

## 验证
1. 后端单测：scan_videos_subs 对含多级子目录 + .ass 的临时目录，产出正确的 title/subtitle_path/category_path、path 空。replace_items 后查 media_item 首行 id=1、按 category_path/title 有序。sqlite_sequence 重置（连续两次 replace，第二次仍从 1）。
2. 真机/命令：执行初始化导入 → 视频菜单三 tab（电影/动漫/剧集）下按目录层级显示条目，名称=字幕文件名。
3. 设置页：配路径只保存、不弹"扫描完成"（不自动扫）。
4. VideoView/SettingsView 分类显示为「剧集」而非「电视剧」。

## 风险
- 低。纯数据/后端逻辑 + 前端字符串改动。唯一注意点：初始化导入的 demo 路径定位（开发期相对路径）与"一次性 vs 可重复执行"——设计为可重复（幂等：每次清库重导），command 触发即可。
