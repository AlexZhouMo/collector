# 去 path 字段 + 无视频文件置灰 实现计划

> REQUIRED SUB-SKILL: subagent-driven-development. Steps use `- [ ]`.

**Goal:** media_item 去 path 列（由 root+category_path+title+.mkv 拼接），去重键改 (kind,category_path,title)；list_media 返回 playable，前端无 mkv 文件置灰不可播。

**设计:** docs/superpowers/specs/2026-09-09-drop-path-playable-design.md

**既有形态:**
- MediaItem{id,kind,category,category_path,title,path,subtitle_path,cover_path,description,platform_ok,exec_path}（model.rs:33）
- ScannedItem 同字段少 id（scanner.rs:116）
- library/mod.rs：insert_one_tx/create_item/update_item/list_items/replace_items 的 SQL 含 path 列，ON CONFLICT(path)
- paths.rs：video_root_key/video_to_absolute/appdata_to_absolute 等
- player_open(app,state,http,path:String)（player/mod.rs:30）：remux(&cache_dir,&path)
- list_media(app,db,kind)（lib.rs:87）：拼绝对
- build_video_item(category,category_path,title,path,subtitle_path,cover_path,description)（lib.rs）
- media_create/media_update 收 path 参数
- 前端 MediaItem 接口含 path；PlayerView 调 api.playerOpen(it.path)；EditDrawer 有 path 占位

---

### Task 1: paths.rs 加 video_abs_path 拼接函数 + 单测
- Create fn `video_abs_path(root:&str, category_path:&str, title:&str)->String`：root 空返回空；否则 root + (category_path 非空 ? "/"+category_path : "") + "/" + title + ".mkv"。
- 单测：root+cp+title→…/cp/title.mkv；cp 空→root/title.mkv；root 空→""。
- `cargo test library::paths`。Commit。

### Task 2: schema 去 path + 重建现有表
- schema.rs：media_item 去 `path TEXT NOT NULL UNIQUE`，加 `UNIQUE(kind,category_path,title)`。
- 一次性重建现有库（Bash sqlite3）：建新表(无path、新唯一约束、含Task前列序 id,kind,category,category_path,title,description,subtitle_path,cover_path,exec_path,platform_ok,scanned_at)→INSERT SELECT(不含path)→DROP→RENAME→建 idx_media_kind。执行前备份 ~/collector.sqlite.bak6。
- 验证列、条数 1288。Commit schema.rs。

### Task 3: 结构体 + library SQL 去 path
- model.rs MediaItem 去 path 字段；scanner.rs ScannedItem 去 path 字段（及其构造处 scan_videos/scan_videos_subs/scan_comics/scan_games/GameManifest 转换——把 path 相关删除；category_path 保留）。
- library/mod.rs：insert_one_tx/create_item/update_item/list_items/replace_items SQL 去 path 列；ON CONFLICT(path)→ON CONFLICT(kind,category_path,title)；list_items SELECT 去 path、构造 MediaItem 去 path。
- 注意 scanner 里原来往 path 塞的绝对/占位值全删；replace_items 排序原按 category_path,title（不依赖 path，OK）。
- cargo build + test 通过。Commit。

### Task 4: list_media 加 playable + player_open 改签名
- lib.rs list_media：MediaItem 加 playable（需 model.rs MediaItem 加 `pub playable: bool`，默认 false，Serialize）。视频：root=settings video_root_key(category)；abs=paths::video_abs_path(root,category_path,title)；playable = !abs.is_empty() && Path::new(&abs).is_file()。非视频 playable=true。subtitle/cover 拼接不变。
- player/mod.rs player_open：签名 path:String → category:String,category_path:String,title:String；内部取 root（需 db state 或 app 拿 settings——player_open 现有 app+state+http，加 db: State<Db>）拼 abs=video_abs_path，remux(&cache_dir,&abs)。文件不存在 remux 会失败返回错误。
- lib.rs 注册 player_open 不变（宏按名）。
- build+test。Commit。

### Task 5: media_create/update 去 path + build_video_item 去 path
- build_video_item 去 path 参数与字段。
- media_create/media_update 去 path 参数；ScannedItem 构造去 path。
- 去重靠新唯一约束。
- build+test。Commit。

### Task 6: 前端去 path + playable 置灰
- ipc.ts：MediaItem 接口去 path、加 `playable: boolean`；playerOpen 改 `(category,categoryPath,title)=>invoke("player_open",{category,categoryPath,title})`。
- PlayerView.ts：openPlayer 调 api.playerOpen(it.category, it.category_path, it.title)。
- EditDrawer.ts：去掉 path 变量与占位逻辑；mediaCreate/mediaUpdate 调用去 path 参数。
- FolderView/TreeView/PosterGrid：视频单元格 it.playable===false 时加 disabled class 且 onclick 不调 onOpen。
- theme.css：.fv-video.disabled/.poster.disabled{opacity:.45;cursor:default;pointer-events 保留右键但取消 hover 上浮}——onclick 内判断 playable 更稳（右键编辑仍可用）。
- ipc mediaUpdate/mediaCreate 签名去 path（invoke 参数去 path）。后端 media_update/create 已去 path 参数（Task5），前后端一致。
- tsc 通过。Commit。

### Task 7: 全回归 + 真机
- cargo test 全绿；tsc 绿。
- 重启：面板视频多为置灰（真实 mkv 不存在）。放一个真实 mkv 到 root/category_path/title.mkv → 该条变亮可双击播放。
- poster_regression.py 若引用 path 需适配（它只用 category/category_path/title，应无关；确认）。

## Self-Review
- 拼接 video_abs_path（T1）→ list_media playable（T4）+ player_open（T4）一致。
- 去 path：schema（T2）/结构体+SQL（T3）/命令（T5）/前端（T6）全链路一致；ON CONFLICT 改 (kind,category_path,title)。
- playable 字段：model MediaItem 加（T4）→ ipc 接口加（T6）→ 前端置灰（T6）。
- 顺序：T1 拼接→T2 表→T3 结构体SQL→T4 读+播放→T5 写→T6 前端→T7 验。表重建（T2）在结构体改（T3）前，避免中途 SQL 列不匹配；但 T2 重建后到 T3 改 SQL 前，若启动 app 会因 list_items SQL 仍 SELECT path 报错——故 T2、T3 应连续完成再启动（计划中不在 T2 后启动）。
