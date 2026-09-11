# covers 按素材类型分子目录 设计文档

**日期**：2026-09-11
**状态**：已确认

## 背景与动机

应用数据目录的 `covers/` 现在扁平存放所有封面（视频手传 `cover_*`、TMDB `tmdb_*`、AniList 漫画 `manga_*`），
游戏封面则直接用游戏目录里的原 `cover.jpg`（不进 covers/）。要按素材类型分子目录，便于管理。

**目标**：`covers/` 下分 `media`/`comic`/`game`；所有写封面的代码写入对应子目录；DB cover_path 存
`covers/<kind>/<文件>`；启动时幂等迁移现有扁平封面并更新 DB；游戏封面纳入（扫描时压缩拷入 covers/game/）。

## 目录与路径规则

- 视频封面（手传/裁剪/TMDB）→ `covers/media/`
- 漫画封面（AniList）→ `covers/comic/`
- 游戏封面（扫描时 cover.jpg 压缩）→ `covers/game/`
- DB cover_path 存相对路径 `covers/media/xxx.jpg` 等。`appdata_to_relative`/`appdata_to_absolute`
  按 `covers/` 段处理，子目录天然兼容——**无需改这两个函数**。
- 前缀保留（cover_/tmdb_/manga_/game_），子目录已区分类型。

## 代码改动（5 处写入点）

1. **import_cover**（视频手传，lib.rs:331）：covers_dir 由 `app_data/covers` 改为 `app_data/covers/media`。
2. **import_cover_cropped**（视频裁剪，lib.rs:345）：同上 `covers/media`。
3. **fetch_posters**（TMDB 视频，lib.rs:418）：covers_dir 改 `covers/media`。
4. **fetch_manga_covers**（AniList 漫画，lib.rs:537）：covers_dir 改 `covers/comic`。
5. **scan_games**（scanner.rs）：改为扫描时读游戏目录的 cover.jpg → `image_proc::to_cover`（居中裁
   2:3 → 500×750 → JPEG q85）→ `image_proc::save_cover(covers/game, "game_")` → cover_path 存相对路径。
   当前是直接存原 cover.jpg 绝对路径，需改。scan_games 需拿到 covers_dir——像 scan_comics 那样由
   scan_root 的 Game 分支传入（scan_root 已能取 app_data_dir）。

`save_cover`/`import_cover` 函数本身不变（接收 covers_dir 参数），只是调用方传的目录变了。
`delete_cover_file`（lib.rs:369）校验 `starts_with(covers/)`——子目录仍在其下，无需改。

## 迁移（启动时幂等）

新增 `migrate_covers_to_subdirs(app_data, db)`，在启动流程调用（仿现有 migrate_subtitles_to_plain）：
- 遍历 `covers/` 根下的**扁平文件**（跳过已在 media/comic/game 子目录的）：
  - `cover_*` / `tmdb_*` → 移到 `covers/media/`
  - `manga_*` → 移到 `covers/comic/`
- 每移一个文件，更新 DB 中所有引用该旧相对路径的 cover_path 为新相对路径（media/comic 表按
  cover_path 值匹配更新）。
- 幂等：已在子目录/已迁移的跳过；文件不存在跳过、不报错；DB 无匹配记录也不报错（文件仍迁移）。
- 游戏封面无需迁移——重扫游戏时自动生成到 covers/game/。

## 数据流

启动 → `migrate_covers_to_subdirs`（移文件 + 改 DB）→ 之后所有写入走子目录。前端显示 cover_path 经
`appdata_to_absolute` 转绝对（不变）。

## 错误处理

- 迁移：单个文件移动失败 / DB 更新失败记日志跳过，不中断启动。
- 目标子目录不存在 → create_dir_all。
- 游戏 cover.jpg 缺失/损坏 → cover_path 留空（现有行为）。

## 测试

- `save_cover`/`import_cover` 传子目录路径的既有测试仍绿（仅调用方目录变化）。
- 新增迁移测试：造扁平 `cover_`/`tmdb_`/`manga_` 文件 + 对应 DB 记录 → 跑迁移 → 断言文件移到对应
  子目录、DB cover_path 更新、幂等重跑不重复移动/不报错。
- scan_games 测试：造带 cover.jpg 的游戏目录 → scan_games(root, covers_dir) → 断言 cover_path 指向
  covers/game/ 且文件落盘。

## 影响与风险

- 后端：lib.rs 5 处 covers_dir、scanner.rs scan_games 重写封面处理、新增迁移函数 + 启动调用、
  scan_root Game 分支传 covers_dir。
- DB：cover_path 值 `covers/xxx` → `covers/<kind>/xxx`（迁移更新）。
- **游戏封面行为变化**：从直接引用原 cover.jpg 改为压缩拷贝到 covers/game/（500×750 JPEG q85），
  与视频/漫画封面格式统一（已与用户确认）。
- **YAGNI**：不改前缀命名、不改 appdata_to_relative/absolute（子目录已兼容）、不动 delete_cover_file
  校验逻辑。
