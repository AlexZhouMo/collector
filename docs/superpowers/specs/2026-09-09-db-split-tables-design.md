# 数据表拆分重构：media/comic/game 三表

## 目标

- media_item 单表拆为三张独立表：media（视频，含 subtitle_path）、comic（漫画）、game（游戏）；comic/game 无 subtitle_path。
- 删字段：kind（表名即类型）、platform_ok、exec_path。
- 删表：watch_state、game_state（及其承载的续播/翻页记忆/启动计数功能）。
- 迁移现有数据按 kind 拆到三表，不丢数据。源码变量/函数/SQL 同步改名。

## 决策汇总

| 项 | 决策 |
|----|------|
| 表 | media_item → media/comic/game 三表 |
| 删字段 | kind、platform_ok、exec_path |
| 删表 | watch_state、game_state（进度/翻页/启动计数功能一并移除） |
| MediaKind 枚举 | 保留，语义改为「选哪张表」的路由标记（不入库） |
| 数据 | 一次性迁移保留（按 kind 分流），执行前备份 |

## 三表结构

```sql
media(id INTEGER PK AUTOINCREMENT, category, category_path, title, description, subtitle_path, cover_path, UNIQUE(category_path,title));
comic(id INTEGER PK AUTOINCREMENT, category, category_path, title, description, cover_path, UNIQUE(category_path,title));
game (id INTEGER PK AUTOINCREMENT, category, category_path, title, description, cover_path, UNIQUE(category_path,title));
settings(key PK, value);  -- 不变
```

## 改动面

### 1. schema.rs
三新表 + settings；不再有 media_item/watch_state/game_state。（新库直接建三表；现有库靠迁移。）

### 2. MediaKind（model.rs）
保留 enum Video/Comic/Game。新增 `table_name()` → "media"/"comic"/"game"（替代原 as_str 的入库 kind 值，用于 SQL 选表）。MediaItem 结构去 kind/platform_ok/exec_path 字段（保留 category/category_path/title/description/subtitle_path/cover_path；comic/game 的 subtitle_path 恒 None）。

### 3. library/mod.rs
- list_items(db, kind)：按 kind.table_name() 选表；SELECT 列去 kind/platform_ok/exec_path；media 含 subtitle_path，comic/game SELECT 时 subtitle_path 置 None。
- create_item/update_item/delete_item/replace_items/insert_one_tx：SQL 表名按 kind 选；列去 kind/platform_ok/exec_path；INSERT/UPDATE media 含 subtitle_path，comic/game 不含。ON CONFLICT(category_path,title)。
- ScannedItem（scanner.rs）：去 platform_ok/exec_path 字段。scan_games 原产出 platform_ok/exec_path → 改为不产这两字段（游戏可启动性运行时判断）。

### 4. 删除进度类功能
- lib.rs：删命令 get_video_pos/set_video_pos/get_comic_page/set_comic_page 及 generate_handler 注册。
- launcher/mod.rs：删 game_state 启动计数（保留启动游戏动作本身；exec 路径运行时按 game.json 现算）。
- 前端 ipc.ts：删 getVideoPos/setVideoPos/getComicPage/setComicPage。
- PlayerView.ts：删续播（getVideoPos/setVideoPos 调用），改为从头播。
- ComicReaderView.ts：删翻页记忆（get/setComicPage），改为从第一页。

### 5. poster/mod.rs
update cover_path 的 SQL：media_item → media（海报仅视频）。测试内 INSERT media_item → media，去 kind 列。

### 6. 游戏可启动性（platform_ok/exec_path 删除的连带）
- GameView 原用 platform_ok 灰置、exec_path 启动。改为：list_media("game") 不返回 platform_ok/exec_path；GameView 启动游戏时后端命令按 game.json 现算 exec 路径（launch_game 改为收 game 标识→查 game.json→取当前平台 exec→启动）。若你的游戏库为空，影响小但代码需自洽。

### 7. 前端其它
- ipc.ts MediaItem 接口去 platform_ok/exec_path。
- list_media(kind) 仍传 kind，后端据此选表。
- GameView 去 platform_ok 灰置逻辑（或改为后端返回一个 playable 计算值——本次简化：不灰置，启动失败时提示）。

## 迁移（一次性，保留数据）

1. 备份 collector.sqlite。
2. 建 media/comic/game 三表。
3. `INSERT INTO media SELECT ... FROM media_item WHERE kind='video'`（列去 kind/platform_ok/exec_path，保留 subtitle_path）。
4. `INSERT INTO comic SELECT id,category,category_path,title,description,cover_path FROM media_item WHERE kind='comic'`。
5. game 同 comic（WHERE kind='game'）。
6. DROP media_item、watch_state、game_state。
7. 1288 视频等数据保留。

## 测试策略

- Rust 单测：db migrations 建三表（改 db/mod.rs 的建表断言）；library CRUD 对三表各测一遍（create/list/update/delete，选表正确、列正确）；poster 分组测试 INSERT 改 media 表。
- 全后端 cargo test 通过；tsc 通过。
- 真机：迁移后视频面板 1288 条正常（拼路径、封面、播放）；漫画/游戏面板正常；编辑/移动/新增正常；续播/翻页记忆已移除（从头播/首页）；游戏能启动（若有游戏数据）。

## 明确不做（YAGNI）

- 不保留 watch_state/game_state 的任何字段（进度/翻页/启动计数功能移除）。
- 不保留 kind/platform_ok/exec_path 列。
- 游戏可启动性不再入库，运行时算。

## 风险

- 贯穿数据层大重构，改动点多（schema/model/library/launcher/poster/前端/迁移）——分步实施、每步 cargo test。
- 迁移不可逆（DROP 旧表）——先备份。
- 游戏 exec_path 删除后启动逻辑重写；游戏库若非空需真机验证启动。
- 续播/翻页记忆移除是预期功能损失（用户已确认）。
