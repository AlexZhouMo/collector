# 存储路径相对化重构

## 目标

数据库中视频/字幕/封面路径改为**相对路径存储**，后端 list_media 读取时拼回绝对路径（前端与播放器零改动）：
- 视频：存分类根下的相对路径（不含分类名），读时按 category 取对应 video_<cat>_root 拼前缀。
- 字幕、封面：存相对 app_data 固定目录（covers/、新建 subtitles/），读时拼 app_data。
- category_path 复用一个字段，去掉分类名首段（存分类内相对目录），前端树逻辑连带改造。
- 重写初始化迁移存量数据；ID 从 1 重置；路径不冗余存分类名。

## 决策汇总

| 项 | 决策 |
|----|------|
| 拼接层 | 后端 list_media 读时拼绝对，前端/播放器零改动 |
| 视频相对基准 | 分类根（video_movie/anime/tv_root）+ 分类内相对路径，不含分类名 |
| category_path | 复用该字段，去掉分类名首段（如「科幻/星战」）；前端树 buildVideoTree 不再 slice(1) |
| 字幕/封面 | 相对 app_data；封面 covers/，字幕新建 subtitles/（同级） |
| 存量迁移 | 重写初始化：导出→按分类(电影>动漫>剧集)+路径排序→转相对逐条插入→ID重置 |

## 数据模型（新存储格式）

| 字段 | 现状 | 新格式 | 读时拼接 |
|----|------|--------|---------|
| category | 电影/动漫/剧集 | 不变 | — |
| category_path | 电影/科幻/星战 | 科幻/星战（去分类名） | 展示树；空=分类根 |
| path（视频） | 绝对 | 科幻/星战/x.mkv（分类根下相对） | video_<cat>_root + "/" + path |
| subtitle_path | 绝对/空 | subtitles/x.ass（相对 app_data）/空 | app_data + "/" + 值 |
| cover_path | 绝对/空 | covers/x.jpg（相对 app_data）/空 | app_data + "/" + 值 |

video_<cat>_root 映射：电影→video_movie_root，动漫→video_anime_root，剧集→video_tv_root。

## 架构与改动

### 1. 新建 subtitles 目录
setup 时 `app_data_dir().join("subtitles")` + create_dir_all，与 covers 同级。

### 2. 后端读时拼接（lib.rs list_media 命令层）
list_media 调 library::list_items 拿相对数据后，遍历拼绝对再返回：
- 视频 path：按 category 取对应 video root（settings 读），root 存在则 `root + "/" + path`，未设置则保持相对（不崩）。
- subtitle_path/cover_path：非空则 `app_data + "/" + 值`；空保持空。
- category_path：不拼（前端树用分类内相对路径；如需显示分类名前端自行补 activeCat）。
- list_items 保持纯粹（只读库返回相对），拼接在命令层做（命令层能同时拿 app + db）。

### 3. 后端写时转相对
所有写库处存相对：
- media_create/media_update：视频路径若为绝对，按其 category 的 root 求相对（strip_prefix root，再去分类名）；字幕/封面若绝对，按 app_data 求相对。已是相对则原样。
- import_cover/import_cover_cropped/海报 save_cover：改为存 `covers/xxx.jpg` 相对（现返回绝对）。
- 字幕规范化：输出拷进 subtitles/，存 `subtitles/xxx.ass` 相对。
- 抽出纯函数：`to_relative_video(abs, category, root)`、`to_relative_appdata(abs, app_data)`、`to_absolute_*` 反向，便于单测。

### 4. 播放器/前端零改动
list_media 已拼绝对 → player_open 收到绝对 path，封面/字幕 convertFileSrc 收到绝对，均无需改。

### 5. 前端树逻辑（category_path 去分类名连锁）
- buildVideoTree(category, items)：去掉 `slice(1)`，category_path 直接按 / 切分构树；根节点 path 为空串（分类内根）。
- TreeNode.path / collectFolderPaths / findNode / 移动 fp：语义改为「分类内相对路径」（不含分类名）；空串=分类根。
- 移动 mediaUpdate(id, category, fp, ...)：fp 为分类内相对路径，直接存 category_path。
- 面包屑等展示需要分类名时，前端在最前补 activeCat（不进数据库）。

### 6. 重写初始化迁移
新命令（或改造 init_from_demo）：
- 备份提醒：迁移前建议手动备份 collector.sqlite（不可逆，ID 重置）。
- 导出现有 media_item 全部行 → 按 category(电影>动漫>剧集 固定序) 再 category_path 排序。
- 逐条转相对：
  - 视频 path：strip video_<cat>_root 前缀 → 相对；再从 category_path 去掉分类名首段。
  - 字幕/封面：strip app_data 前缀 → 相对（保留 covers/ 或 subtitles/ 段）。
- 清库（DELETE + DELETE sqlite_sequence）→ 按排序顺序重插 → ID 从 1。

### 7. 回归脚本适配
poster_regression.py：cover_path 现存相对，脚本读基准/对比时统一（比对相对值，或都拼绝对后比）；重建基准。

## 测试策略

- Rust 纯函数单测：to_relative_video（绝对→剥 root+分类名）、to_absolute_video（相对+category+root→绝对）、to_relative_appdata / to_absolute_appdata（字幕/封面）；空值、root 未设、已是相对等边界。
- buildVideoTree 去 slice 后构树单测：category_path「科幻/星战」正确挂树；空 category_path 挂根。
- 真机：迁移后数据完整；播放/字幕/封面显示正常；面板树/移动/编辑/新增正常；视频 root 改路径后仍能定位（相对存储的价值验证）。

## 明确不做（YAGNI）

- 不改前端播放器/convertFileSrc（后端读时拼，前端拿绝对）。
- 不新建 path 字段（复用 category_path）。
- 不保留分类名冗余存储。
- 迁移不做自动备份（提示用户手动备份）。

## 风险

- 贯穿数据层大改，改动点多（读/写/迁移/前端树/回归脚本）——分步实施、每步测试。
- 迁移不可逆（ID 重置）——迁移前手动备份数据库。
- 视频 root 未设置时读时拼接降级（保持相对/空），不崩。
- category_path 去分类名影响前端树/分组/移动多处——集中在 videoTree.ts + VideoView.ts，需一并改并测。
