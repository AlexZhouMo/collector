# 去 path 字段（拼接推导）+ 无视频文件置灰不可播

## 目标

1. media_item 去掉 path 列，视频绝对路径由 `video_<cat>_root + category_path + title + .mkv` 拼接推导（title=文件名主干）。去重键改用 (kind, category_path, title)。
2. 面板打开文件夹后，视频对应的 mkv 文件不存在时图标置灰、不可点击/双击；存在才可双击播放。

## 决策汇总

| 项 | 决策 |
|----|------|
| path 拼接 | root + "/" + category_path + "/" + title + ".mkv"（category_path 空则省略该段） |
| title 语义 | 等于文件名主干（不含年份剥离，用原始 title） |
| 去重键 | UNIQUE(kind, category_path, title) 替代原 path UNIQUE |
| player_open 入参 | 改收 category + category_path + title，后端拼绝对路径 |
| 可播性 | list_media 顺带对每条视频 Path::exists() 检查，返回 playable:bool |
| 前端置灰 | playable=false → 降透明度 + 不响应双击 |

## 改动方案

### 1. schema.rs + 重建表
- media_item 去掉 `path TEXT NOT NULL UNIQUE`；加 `UNIQUE(kind, category_path, title)`。
- 一次性重建现有表：建新表（无 path、新唯一约束）→ INSERT SELECT（不含 path）→ DROP 旧 → RENAME → 重建索引。

### 2. 结构体 + SQL（library）
- MediaItem、ScannedItem 去掉 `path` 字段。
- insert_one_tx / create_item / update_item / list_items / replace_items 的 SQL 去掉 path 列。
- `ON CONFLICT(path)` → `ON CONFLICT(kind,category_path,title)`。
- 加拼接辅助（paths.rs）：`video_abs_path(root, category_path, title) -> String`。

### 3. list_media（lib.rs）
- 不再拼 path（无字段）。
- 对每条 video：按 category 取 root，`video_abs_path(root, category_path, title)` 得绝对 mkv 路径，`std::path::Path::new(&abs).is_file()` → playable。
- MediaItem 加 `playable: bool` 字段（Serialize），随返回。
- subtitle/cover 拼接不变。

### 4. player_open（player/mod.rs）
- 签名从 `path: String` 改为 `category: String, category_path: String, title: String`。
- 后端取 root（settings video_root_key）拼绝对 mkv 路径 → remux → 返回 {src,duration}。
- 若文件不存在，返回错误（前端本就置灰不会触发，双保险）。

### 5. 前端
- ipc.ts：MediaItem 接口去 path、加 playable:boolean；playerOpen 改传 {category, categoryPath, title}。
- PlayerView：openPlayer 用 it.category/category_path/title 调 playerOpen。
- EditDrawer：去掉 path 相关（本是占位）；保存 mediaCreate/mediaUpdate 不再传 path（后端 build_video_item 去 path）。
- 面板（FolderView/TreeView/PosterGrid）：视频单元格根据 it.playable 加 class（如 `fv-video disabled`），playable=false 时 onclick 不触发 onOpen、加置灰样式。
- theme.css：.fv-video.disabled / .poster.disabled 置灰（opacity:.45，cursor:default，取消 hover 动画）。

### 6. media_create/media_update
- 去掉 path 参数（前端不再传）；build_video_item 去 path。
- 去重靠 (kind,category_path,title) 唯一约束。

## 测试策略

- Rust：video_abs_path 拼接单测（root+category_path+title+.mkv；category_path 空省略段）；list_items 去 path 后字段正确；ON CONFLICT 新键去重。
- 真机：面板视频默认多为置灰（真实 mkv 不存在）；放一个真实 mkv 到对应 root 路径 → 该条变亮可双击播放；播放走 category+path+title 拼接。
- 迁移后 1288 条数据完整（除 path 列）。

## 明确不做（YAGNI）

- 不保留 path 字段/别名。
- 不支持非 mkv 扩展名探测（按需求固定 .mkv）。
- 不做文件监听实时更新 playable（进文件夹/刷新时查一次）。

## 风险

- 去 path 是贯穿改动（表/结构体/SQL/player/前端），需分步 + 每步测。
- 重建表不可逆——执行前备份。
- title 若含非法文件名字符，拼接路径可能与真实文件不符——但 title 即文件名主干，一致。
- 大量置灰是预期正确行为（真实视频未放置时）。
