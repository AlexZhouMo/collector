# 实现计划：数据表拆分重构（media/comic/game 三表）

## Goal

把单表 `media_item` 拆成三张独立表 `media` / `comic` / `game`，删除冗余字段（`kind`、`platform_ok`、`exec_path`），删除进度类表（`watch_state`、`game_state`）及其承载的续播 / 翻页记忆 / 启动计数功能。`MediaKind` 枚举保留，语义从「入库的类型值」改为「选哪张表」的路由标记（新增 `table_name()`）。现有数据一次性脚本迁移保留（1288 条视频等），源码变量 / 函数 / SQL 同步改名。游戏可启动性不再入库，`launch_game` 运行时读该游戏目录的 `game.json` 现算 exec 路径。

## Architecture / 既有形态

- Tauri 2 应用：Rust 后端 `src-tauri/`，vanilla-ts 前端 `src/`。
- DB：单个 SQLite，真实路径 `~/Library/Application Support/com.zhoumo.collector/collector.sqlite`（**不是** spec 假设的 `~/collector.sqlite`；迁移脚本必须用此真实路径）。建表在 `src-tauri/src/db/schema.rs` 的 `MIGRATIONS` 数组，`Db::open` / `Db::open_in_memory`（`src-tauri/src/db/mod.rs`）启动时逐条 `execute_batch`。
- 数据模型：`MediaKind` 枚举 + `MediaItem` struct（`src-tauri/src/library/model.rs`）；`ScannedItem` struct（`src-tauri/src/library/scanner.rs`）为入库中间体。
- CRUD：`src-tauri/src/library/mod.rs` 的 `list_items` / `create_item` / `update_item` / `delete_item` / `replace_items` / `insert_one_tx`，全部硬编码 `media_item` 表名 + `kind` 列 + `platform_ok`/`exec_path` 列。
- 命令层：`src-tauri/src/lib.rs`，含 `list_media`、`media_create/update/delete`、`build_video_item`、进度命令 `get/set_video_pos`、`get/set_comic_page`、`launcher::launch_game`（`src-tauri/src/launcher/mod.rs`），以及 `invoke_handler(generate_handler![...])` 注册列表。
- 海报：`src-tauri/src/poster/mod.rs` 的 `update_cover_path`（UPDATE media_item）+ 三个测试内 `INSERT media_item`。
- 前端：`src/lib/ipc.ts`（`MediaItem` 接口 + api 方法）、`src/views/PlayerView.ts`（续播）、`src/views/ComicReaderView.ts`（翻页记忆）、`src/views/GameView.ts`（platform_ok 灰置 + exec_path 启动）。

## 设计文档

`docs/superpowers/specs/2026-09-09-db-split-tables-design.md`

## 三表目标结构

```sql
media(id INTEGER PK AUTOINCREMENT, category, category_path, title, description, subtitle_path, cover_path, UNIQUE(category_path,title));
comic(id INTEGER PK AUTOINCREMENT, category, category_path, title, description, cover_path, UNIQUE(category_path,title));
game (id INTEGER PK AUTOINCREMENT, category, category_path, title, description, cover_path, UNIQUE(category_path,title));
settings(key PK, value);  -- 不变
```

comic / game 无 `subtitle_path`；三表均无 `kind` / `platform_ok` / `exec_path`。

## 关键顺序与风险提示

严格按 Task 顺序做，每个 Task 结束跑验证 + commit：

1. Task 1（model + scanner 去字段、加 `table_name()`）
2. Task 2（library CRUD 改选表 SQL）
3. Task 3（schema 三表 + db/mod 测试）
4. Task 4（一次性迁移脚本，DROP 旧表不可逆，靠后、先备份）
5. Task 5（删进度功能：lib.rs 命令 + launcher 重写 + 前端）
6. Task 6（poster SQL + 测试）
7. Task 7（前端 ipc / View 收尾）
8. Task 8（全回归）

**危险窗口标注**：Task 1~2 会让代码引用 `media`/`comic`/`game` 表名，但真实 DB 里此时仍是 `media_item`（旧表）。**从 Task 1 开始，到 Task 4 迁移脚本执行完之前，绝对不要启动 app**（列 / 表不匹配会崩，如 player video error）。全程用 `cargo test`（in-memory DB 走 Task 3 后的新 schema）验证，不要 `cargo tauri dev`。Task 3 建三表后 in-memory 测试才通过；Task 4 执行后真实 DB 才可用。因此 Task 3 必须排在 Task 4 前，Task 1~2 的 cargo test 在 Task 3 完成前会因 in-memory schema 仍是旧表而失败——所以 Task 1~2 只做代码改动，其 cargo test 验证顺延到 Task 3 之后统一跑（各 Task checkbox 中标注）。

> 简化策略：Task 1~3 是一组「不可拆分的编译单元」——三者改完后代码才能编译 + cargo test 通过。故 Task 1、Task 2 的「验证」为「代码写完、暂不单独编译」，Task 3 末尾统一 `cargo test`。若想每步可编译，可反序（先 schema 再 model 再 library），但那样 model/library 中途引用不存在的字段同样不可编译；因此采用「三步连改、Task 3 末统一验证」。

---

## Task 1：MediaKind 加 table_name() + 结构体去字段（model.rs、scanner.rs）

**Files**：
- `src-tauri/src/library/model.rs`
- `src-tauri/src/library/scanner.rs`

- [ ] `model.rs`：`impl MediaKind` 新增 `table_name()`，替代 `as_str()` 作选表用（`as_str()` 若无其它引用则删除；grep 确认仅 library/mod.rs 用到，将在 Task 2 改为 `table_name()`）：
  ```rust
  pub fn table_name(&self) -> &'static str {
      match self {
          MediaKind::Video => "media",
          MediaKind::Comic => "comic",
          MediaKind::Game => "game",
      }
  }
  ```
  保留 `from_kind_str`（`list_media`/`scan_root` 仍用它把前端传的 `"video"/"comic"/"game"` 字符串转枚举，注意入参仍是 `"video"` 而非表名 `"media"`）。删除 `as_str()`（其唯一功能是产入库 kind 值，已不入库）。
- [ ] `model.rs`：`struct MediaItem` 删除字段 `pub kind: MediaKind`、`pub platform_ok: bool`、`pub exec_path: Option<String>`。保留 `id`、`category`、`category_path`、`title`、`subtitle_path`、`cover_path`、`description`、`playable`、`video_path`。（comic/game 的 `subtitle_path` 恒 `None`，由 list_items 填充逻辑保证。）
- [ ] `model.rs`：修 tests——`media_kind_serializes_lowercase` 保留（枚举序列化不变）；`from_kind_str_parses_valid_and_rejects_invalid` 保留。可新增断言 `MediaKind::Video.table_name() == "media"` 等。
- [ ] `scanner.rs`：`struct ScannedItem` 删除字段 `pub kind: MediaKind`、`pub platform_ok: bool`、`pub exec_path: Option<String>`。保留 `category`/`category_path`/`title`/`subtitle_path`/`cover_path`/`description`。
  - 说明：`ScannedItem` 删掉 `kind` 后，`replace_items`/`create_item` 的入库表名不能再从 item 读，必须由**调用方传入的 `kind: MediaKind`** 决定（Task 2 处理）。`create_item` 现无 kind 入参——Task 2 给它加 `kind` 形参。
- [ ] `scanner.rs`：`ScannedItem::into_item`（`#[allow(dead_code)]`）同步去 `kind`/`platform_ok`/`exec_path`：
  ```rust
  pub fn into_item(self, id: i64) -> MediaItem {
      MediaItem { id, category: self.category, category_path: self.category_path,
        title: self.title, subtitle_path: self.subtitle_path, cover_path: self.cover_path,
        description: self.description, playable: false, video_path: String::new() }
  }
  ```
- [ ] `scanner.rs`：`scan_videos`（第 46 行 push）去 `kind: MediaKind::Video`、`platform_ok: true`、`exec_path: None` 三行。
- [ ] `scanner.rs`：`scan_videos_subs`（第 98 行 push）同上去三字段。
- [ ] `scanner.rs`：`scan_comics`（第 219 行 push）去 `kind: MediaKind::Comic`、`platform_ok`、`exec_path`。
- [ ] `scanner.rs`：`scan_games`（第 264~318 行）**去 platform_ok/exec_path 计算**：
  - 删 `let rel_exec = ...` 与 `let (platform_ok, exec_path) = match rel_exec {...}` 整段。
  - `GameManifest` 结构体保留 `name`/`description`；`exec_win`/`exec_mac`/`fullscreen` 若 scan 阶段不再用可加 `#[allow(dead_code)]` 或删除——**保留**它们（launch_game 运行时另读 game.json，scan 不需要，故这里三字段可删以免 dead_code 警告；`GameManifest` 只留 `name`/`description`）。实际 launch 时 Task 5 会另定义运行时用的 manifest 结构。
  - push 的 `ScannedItem` 去 `kind: MediaKind::Game`、`platform_ok`、`exec_path`，保留 `category: "游戏"`、`category_path: dir_name`、`title`、`cover_path`、`description`。
  - 修 `scans_game_with_manifest_current_platform` 测试：删 `assert!(items[0].platform_ok)` 与 `assert!(items[0].exec_path...)`，保留 `title`/`items.len()` 断言。
- [ ] `scanner.rs`：修 tests 里所有构造 `ScannedItem { ... }` 的字面量（若有）去三字段。含 `scan_videos`/`scan_comics`/`scan_games` 各自的 test（这些 test 读的是返回值字段，不构造 ScannedItem，主要改 game test 的两条断言）。
- [ ] 验证（暂不单独编译，见「关键顺序」；改动随 Task 2、Task 3 一起在 Task 3 末尾 `cargo test`）。
- [ ] commit：`refactor(model): MediaKind 加 table_name，MediaItem/ScannedItem 去 kind/platform_ok/exec_path 字段`

---

## Task 2：library CRUD 改选表 SQL（library/mod.rs）

**Files**：`src-tauri/src/library/mod.rs`

设计：所有写操作按传入 `kind.table_name()` 拼表名；media 表 INSERT/UPDATE/SELECT 含 `subtitle_path` 列，comic/game 不含。用「按 kind 分支拼 SQL 字符串」而非单条通用 SQL（列集不同）。ON CONFLICT 键改为 `(category_path,title)`（去掉 kind 维度）。

- [ ] `insert_one_tx`：改签名为 `fn insert_one_tx(tx: &Transaction, kind: MediaKind, it: &ScannedItem) -> AppResult<()>`。按 kind 分支：
  - `MediaKind::Video`（表 `media`）：
    ```sql
    INSERT INTO media (category,category_path,title,subtitle_path,cover_path,description)
    VALUES (?1,?2,?3,?4,?5,?6)
    ON CONFLICT(category_path,title) DO UPDATE SET
      category=excluded.category, subtitle_path=excluded.subtitle_path,
      cover_path=excluded.cover_path, description=excluded.description
    ```
    params：`[it.category, it.category_path, it.title, it.subtitle_path, it.cover_path, it.description]`
  - `Comic`/`Game`（表 `comic`/`game`）：无 subtitle_path 列：
    ```sql
    INSERT INTO {table} (category,category_path,title,cover_path,description)
    VALUES (?1,?2,?3,?4,?5)
    ON CONFLICT(category_path,title) DO UPDATE SET
      category=excluded.category, cover_path=excluded.cover_path, description=excluded.description
    ```
  - 用 `let table = kind.table_name();` + `format!` 拼表名（表名来自枚举，非用户输入，无注入风险）。
- [ ] `replace_items(db, kind, items)`：
  - 第 28 行 `DELETE FROM media_item WHERE kind=?1` → `DELETE FROM {table}`（表名 = `kind.table_name()`，整表清空，不再按 kind 过滤）。
  - 第 33 行 `DELETE FROM sqlite_sequence WHERE name='media_item'` → `name='{table}'`。
  - 循环体 `insert_one_tx(&tx, it)?` → `insert_one_tx(&tx, kind, it)?`。
- [ ] `create_item`：改签名为 `pub fn create_item(db: &Db, kind: MediaKind, it: &ScannedItem) -> AppResult<i64>`。按 kind 分支拼 INSERT（同 insert_one_tx 的两套列集，不带 ON CONFLICT）。返回 `last_insert_rowid()`。
  - 注意调用方 `lib.rs::media_create` 传 `MediaKind::Video`（Task 5 处理）。
- [ ] `update_item(db, id, it)`：视频编辑专用，仍只更新 media 表。SQL 表名 `media_item` → `media`（列不变：`category,category_path,title,subtitle_path,cover_path,description WHERE id=?7`）。若要通用可加 kind 参数，但当前仅视频编辑用到，保持只改 media 表 + 注释说明。
- [ ] `delete_item(db, id)`：**问题**——原按 id 从单表删，现三表 id 各自独立可能重复。当前唯一调用方 `lib.rs::media_delete` 仅用于视频（编辑面板删视频），故改 `DELETE FROM media WHERE id=?1` + 注释「仅视频删除」。（若未来 comic/game 也要删，需加 kind 参数；YAGNI，本次只改 media。）
- [ ] `list_items(db, kind)`：按 kind 分支：
  - `Video`（media 表）：`SELECT id,category,category_path,title,subtitle_path,cover_path,description FROM media ORDER BY category_path, title`；构造 `MediaItem { id, category, category_path, title, subtitle_path: r.get(4)?, cover_path, description, playable:false, video_path:String::new() }`。
  - `Comic`/`Game`：`SELECT id,category,category_path,title,cover_path,description FROM {table} ORDER BY category_path, title`；构造时 `subtitle_path: None`。
  - 删除原从 row 读 `kind_s` 再 match 的整段（第 111~116 行），MediaItem 已无 kind 字段。
- [ ] `library/mod.rs` 文件头注释（第 13~16 行）更新：去掉「watch_state/game_state 随 CASCADE」等已删表的描述，改为「清空该 kind 对应表后重插」。第 44、80 行注释同步。
- [ ] 修 tests（第 139~237 行）：
  - `sample()`（第 145 行）去 `kind: MediaKind::Video`、`platform_ok`、`exec_path` 字段。
  - `replace_only_affects_its_kind`（第 199 行）：`comic.kind = MediaKind::Comic` 这行删除（ScannedItem 已无 kind）；comic 由 `replace_items(&db, MediaKind::Comic, &[comic])` 的 kind 参数决定入 comic 表。断言不变（video/comic 各 1 条，互不影响——因现在是不同表，天然隔离）。
  - `create_returns_incrementing_id`、`update_changes_fields`、`delete_removes` 等：`create_item(&db, &sample(...))` → `create_item(&db, MediaKind::Video, &sample(...))`。
  - `replace_resets_id_to_one_and_orders`（第 181 行）：`SELECT ... FROM media_item` → `FROM media`。
  - `update_changes_fields`（第 216 行）：`SELECT id FROM media_item LIMIT 1` → `FROM media`。
- [ ] 验证：随 Task 3 末尾统一 `cargo test`（见关键顺序说明）。
- [ ] commit：`refactor(library): CRUD 按 kind.table_name 选表，去 kind/platform_ok/exec_path 列`

---

## Task 3：schema 建三表 + db/mod 测试（schema.rs、db/mod.rs）

**Files**：
- `src-tauri/src/db/schema.rs`
- `src-tauri/src/db/mod.rs`

- [ ] `schema.rs`：整个 `MIGRATIONS` 数组重写为建三表 + settings（删 media_item / idx_media_kind / watch_state / game_state）：
  ```rust
  pub const MIGRATIONS: &[&str] = &[
      "CREATE TABLE IF NOT EXISTS media (
          id INTEGER PRIMARY KEY AUTOINCREMENT,
          category TEXT NOT NULL, category_path TEXT NOT NULL, title TEXT NOT NULL,
          description TEXT, subtitle_path TEXT, cover_path TEXT,
          UNIQUE(category_path,title));",
      "CREATE TABLE IF NOT EXISTS comic (
          id INTEGER PRIMARY KEY AUTOINCREMENT,
          category TEXT NOT NULL, category_path TEXT NOT NULL, title TEXT NOT NULL,
          description TEXT, cover_path TEXT,
          UNIQUE(category_path,title));",
      "CREATE TABLE IF NOT EXISTS game (
          id INTEGER PRIMARY KEY AUTOINCREMENT,
          category TEXT NOT NULL, category_path TEXT NOT NULL, title TEXT NOT NULL,
          description TEXT, cover_path TEXT,
          UNIQUE(category_path,title));",
      "CREATE TABLE IF NOT EXISTS settings (key TEXT PRIMARY KEY, value TEXT NOT NULL);",
  ];
  ```
  说明：`MIGRATIONS` 数组用 `CREATE TABLE IF NOT EXISTS`，每次启动幂等重跑安全；**不含 DROP / INSERT 迁移语句**（迁移是 Task 4 一次性脚本，不进数组，避免每次启动重跑）。
- [ ] `db/mod.rs` 测试 `migrations_create_tables`（第 37~45 行）：断言表名集合改为 `('media','comic','game','settings')`，count 仍 == 4。
- [ ] `db/mod.rs` 测试 `foreign_keys_cascade_delete`（第 47~72 行）：该测试依赖 `media_item` + `watch_state` + CASCADE，现两表均删——**整个测试删除**（无级联关系可测）。`foreign_keys` pragma 仍 ON，可保留一个精简断言仅验证 pragma：新增/改写为
  ```rust
  #[test]
  fn foreign_keys_pragma_on() {
      let db = Db::open_in_memory().unwrap();
      let conn = db.0.lock().unwrap();
      let fk: i64 = conn.query_row("PRAGMA foreign_keys", [], |r| r.get(0)).unwrap();
      assert_eq!(fk, 1);
  }
  ```
- [ ] 验证：`cd src-tauri && cargo test`（此时 Task 1~3 代码齐全，应全绿；重点看 library / model / scanner / db 各测试）。
- [ ] commit：`refactor(schema): media_item 拆为 media/comic/game 三表，删 watch_state/game_state`

---

## Task 4：一次性迁移脚本（保留数据，DROP 旧表不可逆，先备份）

**Files**：无源码改动；执行一次性 shell 脚本操作真实 DB。

> 前置：Task 1~3 已 commit（代码指向新表）。此步执行前**真实 DB 仍是旧结构**，app 未启动。执行后真实 DB 变为新三表，app 可正常启动。

- [ ] 确认 app 已退出（避免 SQLite 锁）：`pgrep -fl collector` 无输出。
- [ ] 备份（真实路径，非 spec 的 ~/collector.sqlite）：
  ```bash
  DB="$HOME/Library/Application Support/com.zhoumo.collector/collector.sqlite"
  cp "$DB" "$HOME/collector.sqlite.bak_split"
  ```
- [ ] 迁移（一次性 `sqlite3` 脚本，建三表 → 按 kind 分流 INSERT → DROP 旧表）：
  ```bash
  sqlite3 "$DB" <<'SQL'
  BEGIN;
  CREATE TABLE IF NOT EXISTS media (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    category TEXT NOT NULL, category_path TEXT NOT NULL, title TEXT NOT NULL,
    description TEXT, subtitle_path TEXT, cover_path TEXT,
    UNIQUE(category_path,title));
  CREATE TABLE IF NOT EXISTS comic (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    category TEXT NOT NULL, category_path TEXT NOT NULL, title TEXT NOT NULL,
    description TEXT, cover_path TEXT, UNIQUE(category_path,title));
  CREATE TABLE IF NOT EXISTS game (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    category TEXT NOT NULL, category_path TEXT NOT NULL, title TEXT NOT NULL,
    description TEXT, cover_path TEXT, UNIQUE(category_path,title));
  INSERT INTO media (id,category,category_path,title,description,subtitle_path,cover_path)
    SELECT id,category,category_path,title,description,subtitle_path,cover_path
    FROM media_item WHERE kind='video';
  INSERT INTO comic (id,category,category_path,title,description,cover_path)
    SELECT id,category,category_path,title,description,cover_path
    FROM media_item WHERE kind='comic';
  INSERT INTO game (id,category,category_path,title,description,cover_path)
    SELECT id,category,category_path,title,description,cover_path
    FROM media_item WHERE kind='game';
  DROP TABLE IF EXISTS watch_state;
  DROP TABLE IF EXISTS game_state;
  DROP TABLE IF EXISTS media_item;
  COMMIT;
  SQL
  ```
  说明：保留原 id（三表各自独立，跨表 id 允许重复，无碍——list_items 按表查）；video 保留 subtitle_path；comic/game 不带 subtitle_path。settings 表原样不动。
- [ ] 验证迁移结果：
  ```bash
  sqlite3 "$DB" "SELECT count(*) FROM media; SELECT count(*) FROM comic; SELECT count(*) FROM game;"
  sqlite3 "$DB" "SELECT name FROM sqlite_master WHERE type='table' ORDER BY name;"  # 应为 comic,game,media,settings（+可能 sqlite_sequence）
  ```
  确认 media 数量与迁移前 `kind='video'` 数量一致（预期约 1288）。
- [ ] 回滚说明（写进 commit body 或此处备注，不执行）：如需回滚 `cp "$HOME/collector.sqlite.bak_split" "$DB"`。
- [ ] commit：无源码改动则跳过 commit；若把脚本落盘到 `scripts/` 则 commit `chore(db): 一次性迁移脚本 media_item 拆三表`（可选，按项目习惯）。

---

## Task 5：删进度功能（lib.rs 命令 + launcher 重写 + 前端进度调用）

**Files**：
- `src-tauri/src/lib.rs`
- `src-tauri/src/launcher/mod.rs`
- `src/lib/ipc.ts`
- `src/views/PlayerView.ts`
- `src/views/ComicReaderView.ts`

### 5a. lib.rs 删进度命令 + 修 build_video_item + create/delete 传 kind

- [ ] `lib.rs`：删除四个进度命令函数：`set_comic_page`（第 118~132）、`get_comic_page`（第 134~146）、`set_video_pos`（第 148~162）、`get_video_pos`（第 164~176）。
- [ ] `lib.rs`：`generate_handler![...]`（第 474~502）删除注册：`set_comic_page`、`get_comic_page`、`set_video_pos`、`get_video_pos`。
- [ ] `lib.rs`：`build_video_item`（第 178~199）去 `kind: MediaKind::Video`、`platform_ok: true`、`exec_path: None`（ScannedItem 已无这些）。返回 `ScannedItem { category, category_path, title, subtitle_path, cover_path, description }`。注释同步。
- [ ] `lib.rs`：`media_create`（第 253 行）调用 `library::create_item(&db, MediaKind::Video, &it)`（Task 2 create_item 加了 kind 参数）。
- [ ] `lib.rs`：`media_delete`（第 256~259）不变（delete_item 只删 media，Task 2 已改注释）。
- [ ] `lib.rs`：确认 `use library::model::{MediaItem, MediaKind};` 仍需要 MediaKind（list_media、media_create、scan_root 用）。

### 5b. launcher 重写（去 game_state；exec 运行时按 game.json 现算）

- [ ] `launcher/mod.rs`：`launch_game` 改为收游戏标识（用 `category_path` + `title`），运行时定位游戏目录读 `game.json` 取当前平台 exec 启动。游戏目录 = `game_root` + `category_path`（scan_games 的 `category_path = dir_name`，即游戏根下的一级子目录名；`category` 恒 "游戏"）。
  - settings key：`scan_root` 用 `{kind}_root`，游戏为 `game_root`（`src-tauri/src/lib.rs::scan_root` + `set_root/get_root` 用 `{kind}_root`；前端 GameSettings 若有另需核对，本次假定为 `game_root`）。
  - 新签名（camelCase）：
    ```rust
    #[tauri::command(rename_all = "camelCase")]
    pub fn launch_game(db: tauri::State<Db>, category_path: String, title: String) -> AppResult<()>
    ```
  - 实现步骤：
    1. `let root = crate::settings::get(&db, "game_root")?.ok_or_else(|| AppError::Invalid("游戏目录未设置".into()))?;`
    2. `let dir = Path::new(&root).join(&category_path);`（category_path 即游戏子目录名）
    3. 读 `dir.join("game.json")`，`serde_json` 解析出 `exec_win`/`exec_mac`：
       ```rust
       #[derive(serde::Deserialize)]
       struct GameExec { exec_win: Option<String>, exec_mac: Option<String> }
       ```
       （launcher 自定义此结构；`title` 参数目前仅用于错误提示/未来匹配，若无匹配需求可只用 category_path 定位——保留 title 参数以便报错信息含标题。）
    4. 按平台取 rel exec：`cfg!(target_os="windows")` → exec_win，else exec_mac；`None` 则返回 `AppError::Invalid(format!("{title}: 本平台无可执行文件"))`。
    5. `let exec_path = dir.join(rel);` 若 `!exec_path.exists()` 返回 `AppError::NotFound(...)`。
    6. 启动逻辑复用原 macOS `.app` → `open`、其余 `Command::new` + `current_dir(dir)`（Windows 用 dir 作 cwd；macOS 非 .app 也可加 current_dir(&dir)）。
  - **删除 game_state 写入**（原第 31~42 行整段 `INSERT INTO game_state ...`）。函数不再需要 `db` 里的 game_state；但仍需 `db` 取 `game_root` settings，故 `db: State<Db>` 参数保留。
  - `use` 调整：不再需要 `std::time`；保留 `Path`、`Command`、`AppError`。
- [ ] `launcher/mod.rs`：若原有针对 launch_game 的测试则同步（当前文件无 test，跳过）。

### 5c. 前端进度调用移除

- [ ] `PlayerView.ts`：删续播——
  - 第 72~73 行 `const resume = await api.getVideoPos(...)` + `if (resume > 5 ...) video.currentTime = resume;` 删除。
  - `video.ontimeupdate`（第 108 行）内 `api.setVideoPos(it.id, video.currentTime)` 删除（保留进度条 UI 更新逻辑）。
  - `cleanup`（第 115 行）内 `if (video.currentTime > 0) api.setVideoPos(...)` 删除。
  - 结果：从头播放，不写进度。
- [ ] `ComicReaderView.ts`：删翻页记忆——
  - 第 15 行 `let idx = Math.min(await api.getComicPage(it.id), ...)` → `let idx = 0;`（从第一页）。
  - `renderPage`（第 37 行）内 `await api.setComicPage(it.id, idx)` 删除。
- [ ] `ipc.ts`：`api` 对象删除 `setComicPage`、`getComicPage`、`setVideoPos`、`getVideoPos` 四个方法（第 34~37 行）。（`MediaItem` 接口的 platform_ok/exec_path 在 Task 7 删。）
- [ ] 验证：
  - `cd src-tauri && cargo test`（后端全绿）。
  - `cd .. && npx tsc --noEmit`（此时 GameView 仍引用 it.platform_ok/it.exec_path 且 ipc.ts MediaItem 接口尚保留这些字段——**Task 7 前 tsc 应仍通过**；PlayerView/ComicReaderView 已不引用被删 api，OK）。若 launchGame 签名尚未改（Task 7 改 ipc + GameView），此处 tsc 仍绿。
- [ ] commit：`refactor: 删续播/翻页记忆/启动计数，launch_game 改运行时读 game.json`

---

## Task 6：poster SQL + 测试（poster/mod.rs）

**Files**：`src-tauri/src/poster/mod.rs`

- [ ] `update_cover_path`（第 106~114）：`UPDATE media_item SET cover_path=?1 WHERE id=?2` → `UPDATE media SET cover_path=?1 WHERE id=?2`（海报仅视频）。
- [ ] tests `mk()`（第 121~135）：构造 `MediaItem` 去 `kind: LibKind::Video`、`platform_ok`、`exec_path` 字段；`use crate::library::model::MediaKind as LibKind;` 若不再引用则删除该 use。
- [ ] tests 三处 `INSERT INTO media_item (id,kind,category,category_path,title) VALUES (...,'video',...)`（第 144、176、207 行）→ `INSERT INTO media (id,category,category_path,title) VALUES (...)`（去 kind 列与 'video' 值，参数相应减一）。
- [ ] tests 两处 `SELECT cover_path FROM media_item WHERE id=...`（第 197、198 行）→ `FROM media`。
- [ ] 验证：`cd src-tauri && cargo test`（poster 三测试 + 全后端绿）。
- [ ] commit：`refactor(poster): cover_path 更新与测试 INSERT 改 media 表，去 kind 列`

---

## Task 7：前端 ipc / GameView 收尾（ipc.ts、GameView.ts）

**Files**：
- `src/lib/ipc.ts`
- `src/views/GameView.ts`

- [ ] `ipc.ts`：`MediaItem` 接口删除 `kind`、`platform_ok`、`exec_path` 三个字段（第 4、12、13 行）。保留 `id/category/category_path/title/subtitle_path/cover_path/description/playable/video_path`。
  - 注意：若前端别处用 `it.kind`，grep 确认（当前 grep 无前端 `.kind` 使用；listMedia 传参用字面量 "video"/"comic"/"game"，不依赖字段）。
- [ ] `ipc.ts`：`launchGame` 方法改签名匹配新后端命令（收 categoryPath + title，去 execPath）：
  ```ts
  launchGame: (categoryPath: string, title: string) =>
    invoke<void>("launch_game", { categoryPath, title }),
  ```
- [ ] `GameView.ts`：去 platform_ok 灰置——
  - 第 13 行 `game-card ... ${it.platform_ok ? "" : "disabled"}` → 去掉 disabled 逻辑，保留 `game-card glass card-hover`。
  - 第 18 行 `${it.platform_ok ? "" : "<div class=game-na>本平台不可用</div>"}` 删除。
  - 第 23 行双击启动改为：`c.ondblclick = () => { api.launchGame(it.category_path, it.title).catch(e => alert("启动失败：" + e)); };`（启动失败时 alert 提示，替代原灰置预防）。
- [ ] 验证：`npx tsc --noEmit`（前端全绿；GameView 不再引用 platform_ok/exec_path，launchGame 调用与新签名一致）。
- [ ] commit：`refactor(frontend): MediaItem 去 platform_ok/exec_path，GameView 去灰置，launchGame 改收 categoryPath+title`

---

## Task 8：全回归验证

**Files**：无改动，仅验证。

- [ ] `cd src-tauri && cargo test` 全绿（db/library/model/scanner/poster）。
- [ ] `cd .. && npx tsc --noEmit` 全绿。
- [ ] `cargo build`（或 `cargo tauri build` 前置编译）无 warning 遗留（尤其 dead_code：确认删干净的 as_str/exec 字段无残留引用）。
- [ ] **真机验证（迁移已在 Task 4 完成，此时才可启动 app）**：`cargo tauri dev`
  - 视频面板：约 1288 条正常显示；封面显示；双击可播（player 不再 video error 4）；从头播放（无续播）。
  - 编辑/移动/新增视频：media_update / media_create / media_delete 正常，写入 media 表。
  - 漫画面板：列表正常；进入阅读器从第一页开始（无翻页记忆）。
  - 游戏面板：列表正常（无灰置）；若有游戏数据，双击能启动（launch_game 运行时读 game.json）；无游戏则面板空但不报错。
  - 海报抓取：fetch_posters 正常回填 media 表 cover_path。
- [ ] 最终 commit（如有收尾）：`chore: db-split-tables 重构完成，全回归通过`

---

## 自查对照（spec 每条覆盖）

- media_item → media/comic/game 三表：Task 3（schema）+ Task 4（迁移）+ Task 2（CRUD 选表）✓
- 删 kind：Task 1（model/scanner struct）+ Task 2（SQL）+ Task 3（schema）+ Task 7（ipc）✓
- 删 platform_ok/exec_path：Task 1（struct + scan_games 计算）+ Task 2（SQL）+ Task 7（ipc/GameView）✓
- 删 watch_state/game_state 表：Task 3（schema）+ Task 4（DROP）✓
- 删续播/翻页/启动计数功能：Task 5a（lib 命令）+ 5b（launcher）+ 5c（前端）✓
- MediaKind 保留 + table_name()：Task 1 ✓
- 迁移保留数据 + 备份：Task 4（真实 DB 路径 + bak_split）✓
- 源码同步改名：Task 1~7 全覆盖 ✓
- 游戏 exec 运行时算：Task 5b（launch_game 读 game.json）+ Task 7（GameView 调用）✓

## 类型/函数名一致性检查点

- `MediaKind::table_name()` → "media"/"comic"/"game"；`from_kind_str` 入参仍是 "video"/"comic"/"game"（前端 kind 字符串），二者语义不同勿混。
- `MediaItem` / `ScannedItem` 字段集：`category, category_path, title, subtitle_path, cover_path, description`（+ MediaItem 独有 `id, playable, video_path`）。
- `create_item(db, kind, it)`、`insert_one_tx(tx, kind, it)`、`replace_items(db, kind, items)` 均带 kind；`update_item(db, id, it)` / `delete_item(db, id)` 仅 media。
- `launch_game(db, categoryPath, title)`（Rust rename_all camelCase）↔ 前端 `api.launchGame(categoryPath, title)` ↔ `invoke("launch_game", { categoryPath, title })` 三处一致。

## 明确不做（YAGNI）

- 不保留 watch_state/game_state 任何字段（进度/翻页/启动计数移除）。
- 不保留 kind/platform_ok/exec_path 列。
- 游戏可启动性不入库，运行时算。
- delete_item/update_item 不加 kind 参数（仅视频用到）。
