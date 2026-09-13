# 字幕存储重构 + 「字幕批量校准」工具 设计

日期：2026-09-10
状态：设计已确认，待写实现计划

## 背景与问题

当前字幕存储用「源路径 hash 扁平命名」：字幕拷到 `<app_data>/subtitles/sub_<hash>.ass`，media 表用 `subtitle_path` 列记这个相对路径。这套机制有两个问题：
1. **hash 命名与逻辑位置脱钩**：字幕文件名不可读，和它属于哪部片子无关联，靠 `subtitle_path` 列间接绑定。文件夹重命名/移动改的是 `category_path`，但字幕文件名和 `subtitle_path` 不跟随，造成「数据库指向的字幕」与「实际逻辑归属」错位（错误关联）。
2. **播放器字幕库与校准工具输出割裂**：工具箱里的「字幕标准化」输出到用户选的任意目录，和播放器实际使用的 `<app_data>/subtitles/` 是两套文件。

同时工具箱 UI 需要调整：「字幕标准化」改名「字幕批量校准」、移到「影视海报生成」下方、标题样式与海报生成统一、输入目录可配（默认 `docs/subtitles`）、输出固定为应用字幕目录。

## 目标

1. **字幕路径无状态推导**：字幕文件固定存 `<app_data>/subtitles/<category>/<category_path>/<title>.ass`，与视频磁盘路径（`root/category_path/title.mkv`）同构。播放器按此规则推导字幕位置，**不再存 `subtitle_path` 列**。文件夹重命名/移动改 `category_path` 时，字幕位置自然跟随（复用已有级联逻辑，只需扩展到磁盘字幕目录）。
2. **播放器与校准共用同一字幕文件**：校准工具输出到同一推导路径，播放即用。
3. **一次性迁移**：启动时把旧 hash 字幕按其 media 的 category/path/title 归位到新明文路径，删除 `subtitle_path` 列（迁移前备份 sqlite）。
4. **工具箱 UI 调整**：改名、移位、样式统一、输入可配、输出固定。本轮校准逻辑复用现有 `normalize_subtitles` 引擎（完整 11 条规则为独立的下一周期）。

## 决策汇总

| 项 | 决策 |
|----|------|
| 字幕路径 | 推导：`<app_data>/subtitles/<category>/<category_path>/<title>.ass`（不含 category_path 时省略中段）；**去掉 `subtitle_path` 列** |
| 播放取字幕 | player_open 按 category/category_path/title 推导路径，文件存在则返回，否则无字幕 |
| 输入目录 | settings 键 `subtitle_input_dir`，可配、记住；默认 `docs/subtitles`（相对项目，首次可为空让用户选） |
| 输出目录 | 固定 `<app_data>/subtitles/`，按输入相对结构写入 |
| 迁移 | 启动时自动一次：备份 `collector.sqlite`→ 逐条把 hash 字幕移到推导路径 → schema 迁移删列。容错：失败条目记日志跳过，不中断 |
| 文件名 | 不做 sanitize，直接用 category/path/title 拼（含非法字符的极少数条目迁移失败则跳过） |
| UI | 「字幕标准化」→「字幕批量校准」，移到「影视海报生成」下方，用 `.setting-card` 样式与海报生成一致 |
| 本轮校准引擎 | 复用现有 `normalize_subtitles`（全角标点/统一 ASS 头等）；完整规则下轮 |
| 不做(YAGNI) | 完整 11 条校准规则、字幕 sanitize、手动迁移按钮、EditDrawer 里重新导入字幕 |

## 架构

### 字幕路径推导（新核心）

新增纯函数（Rust，`library::paths` 内）：
```rust
/// 字幕在 app_data 下的相对路径：subtitles/<category>/<category_path>/<title>.ass
/// category_path 为空则省略中段。用于播放推导、校准输出、迁移归位——三处共用。
pub fn subtitle_rel_path(category: &str, category_path: &str, title: &str) -> String;
/// 拼成绝对路径：<app_data>/subtitles/...
pub fn subtitle_abs_path(app_data: &str, category: &str, category_path: &str, title: &str) -> String;
```
例：`subtitle_rel_path("电影", "动作/古墓丽影", "[2001].古墓丽影")` = `subtitles/电影/动作/古墓丽影/[2001].古墓丽影.ass`。

> 注意：`category_path` 现有语义是「分类内相对路径，不含分类名」（如 `动作/古墓丽影`）。字幕路径带上 category 前缀（`subtitles/电影/...`），使不同分类同名目录不冲突。视频路径靠各自 root 区分分类，字幕都在一个 subtitles 根下，故需 category 段。

### 播放取字幕（改 player_open）

`player/mod.rs` 的 `resolve_subtitle` 从「查 `subtitle_path` 列」改为「推导路径 + 判断文件存在」：
```rust
fn resolve_subtitle(app_data: &str, category: &str, category_path: &str, title: &str) -> Option<String> {
    let abs = crate::library::paths::subtitle_abs_path(app_data, category, category_path, title);
    if std::path::Path::new(&abs).is_file() { Some(abs) } else { None }
}
```
不再需要 `db` 查询。`player_open` 已有 category/category_path/title 参数与 app_data，直接调用。

### 去掉 subtitle_path 列的连带改动

- **schema**：`MIGRATIONS` 数组追加一条 `ALTER TABLE media DROP COLUMN subtitle_path;`（SQLite 3.35+ 支持；用幂等保护——见迁移）。
- **library CRUD**（`library/mod.rs`）：`replace_items` / `update_item` / `create_item` / `list_items` 的 SQL 去掉 `subtitle_path`；`ScannedItem`/`MediaItem` 模型去掉该字段。
- **命令签名**（`lib.rs`）：`media_update` / `media_create` 去掉 `subtitle_path` 参数（7→6 参）；`init_from_demo` 里对 subtitle_path 的绝对化（lib.rs:98-99）移除——demo 导入不再写字幕路径列（已无此列），从 demo 目录带来的字幕由后续「校准工具输出」或首启迁移落位到推导路径，不在 demo 导入时处理。
- **前端**：`ipc.ts` 的 `MediaItem` 去 `subtitle_path`；`mediaUpdate`/`mediaCreate` 去 subtitlePath 参数；`MoveDialog.ts`、`EditDrawer.ts` 调用处相应去参。
- **EditDrawer 字幕行**：改为**只读显示推导路径是否存在**（「字幕：<推导相对路径>（存在/无）」），移除原 subtitle_path 展示与透传；不提供导入（YAGNI）。

### 一次性迁移（启动时）

在 `Db::open` 之后、命令注册之前（`lib.rs` setup 内）跑一次迁移函数 `migrate_subtitles_to_plain(app_data, db)`：
```
if media 表仍有 subtitle_path 列（PRAGMA table_info 检测）:
    1. 备份 collector.sqlite → collector.sqlite.bak（若 .bak 已存在则跳过备份，避免覆盖）
    2. 查所有 subtitle_path 非空的 media 行 (category, category_path, title, subtitle_path)
    3. 逐条：src = <app_data>/<subtitle_path>（旧 hash 文件）；
             dst = subtitle_abs_path(app_data, category, category_path, title)；
             src 是文件 → 创建 dst 父目录 → rename(src, dst)（失败记日志跳过）
    4. ALTER TABLE media DROP COLUMN subtitle_path
```
幂等：迁移以「subtitle_path 列是否存在」为闸门，列删掉后再启动不再迁移。旧 hash 文件迁移后 rename 走（原地消失）；迁移不到 media 的孤儿 hash 文件留在原地不动（不误删）。

### 工具箱 UI（`NormalizeView.ts`）

- 「字幕标准化」卡片：改名标题为「字幕批量校准」，从工具箱顶部**移到「影视海报生成」卡片下方**，改用 `.setting-card` / `.setting-card-title` 结构（与海报生成一致，替换原 `<h3>`）。
- 输入目录：一个「选择输入目录」按钮 + 路径显示；值存 settings `subtitle_input_dir`，进入视图时预填（`api.getSetting` 或复用现有 settings 命令）；默认展示 `docs/subtitles`。
- 移除「选择输出目录」按钮（输出固定）。显示固定输出说明「输出到应用字幕库」。
- 「开始」：调 `normalizeSubtitles(inputDir)`（改为单参，输出后端固定推导）；报告展示不变。

### 后端命令 `normalize_subtitles` 调整

- 签名从 `(in_dir, out_dir)` 改为 `(in_dir)`（app 注入拿 app_data，输出固定 `<app_data>/subtitles/`）。
- `run_subtitle_normalize` 的 out_dir 传 `<app_data>/subtitles/`，保持「按输入相对结构写入」。
- 新增 settings 读写输入目录：命令 `get_subtitle_input_dir` / `set_subtitle_input_dir`（或复用通用 settings get/set）。

## 数据流

1. 启动 → Db::open → `migrate_subtitles_to_plain`（首次：备份+归位+删列；之后：no-op）。
2. 播放 → player_open → resolve_subtitle 推导路径 + 文件存在判断 → 返回给前端 SubtitleRenderer。
3. 校准 → 工具箱选输入目录（默认 docs/subtitles，存 settings）→ 开始 → 后端 normalize_subtitles 读输入、写 `<app_data>/subtitles/<相对结构>` → 报告。
4. 重命名/移动文件夹 → 已有 rename_folder / media_update 改 category_path → 字幕路径推导自然跟随（**但磁盘上的字幕文件也需跟随移动**——见风险/联动）。

## 联动：重命名/移动与字幕文件

去掉 subtitle_path 后，字幕位置 = 推导路径。重命名文件夹（`rename_folder`）现在只 rename 磁盘视频目录 + 改库 category_path。**字幕在独立的 `<app_data>/subtitles/` 下，需同样 rename**：
- `rename_folder` 增加：把 `<app_data>/subtitles/<category>/<old_path>` rename 到 `.../<new_path>`（存在才动，容错跳过）。
- 移动单条视频（media_update 改 category_path）：把该条字幕从旧推导路径 rename 到新推导路径（存在才动）。
本轮将这两处字幕跟随纳入（否则改完名字幕就丢），作为存储重构的一部分。

## 错误处理

- 迁移某条失败（src 不存在/dst 父目录建失败/rename 失败/含非法字符）→ 记日志跳过，继续下一条，不中断启动。
- 备份：若 `.bak` 已存在不覆盖（保留最早备份）。
- 播放推导路径无文件 → 无字幕（正常，不报错）。
- 校准输入目录未配置/不存在 → 前端提示选择。
- 字幕跟随 rename：目标已存在或源不存在 → 跳过（容错）。

## 测试策略

- **后端纯函数单测**（`cargo test`）：
  - `subtitle_rel_path`：含 category_path、空 category_path 两种。
  - 迁移逻辑：内存库 + 临时目录，seed 几条带 hash 字幕的 media，跑迁移，断言文件移到推导路径、subtitle_path 列被删、孤儿文件不动、失败条目跳过不中断。
  - resolve_subtitle：文件存在返回绝对路径、不存在返回 None。
- **前端**：`tsc --noEmit`、`npm run build` 通过。
- **真机**（`npm run tauri dev`）：
  1. 启动触发迁移：`<app_data>/subtitles/` 下出现 `电影/动作/古墓丽影/[2001].古墓丽影.ass` 等明文结构；`collector.sqlite.bak` 生成；DB 无 subtitle_path 列。
  2. 播放一部有字幕的片 → 字幕正常显示（走推导路径）。
  3. 工具箱：卡片在海报生成下方、标题「字幕批量校准」样式一致；选输入目录（默认 docs/subtitles）、开始 → 输出到 app_data/subtitles 对应结构、报告正常。
  4. 重命名一个文件夹 → 该目录下视频仍能播放字幕（字幕目录已跟随 rename）。
  5. 移动一条视频到别的文件夹 → 字幕跟随、仍能播放。
  6. 重启 → 不重复迁移（no-op）。

## 明确不做（YAGNI）

- 完整 11 条校准规则（合并分离行、对话 `-`、跨条 `...`、非对话括号、外语方括号、歌曲 `#`、特殊字符表、OCR 纠错、引号「」配对、时间线交叉检测增强）——独立的下一个 brainstorm 周期。
- 文件名 sanitize。
- 手动迁移按钮。
- EditDrawer 里重新导入/替换字幕。
- 漫画/游戏的字幕（无字幕概念）。

## 风险

- **删列的连带面广**：subtitle_path 遍布 model/library/lib.rs/player/前端 ipc/MoveDialog/EditDrawer。实现计划需逐点覆盖，靠 `cargo build` + `tsc` 兜底找漏。
- **迁移不可逆**：先备份 sqlite；旧字幕 rename（非 copy）会移走原文件，但有 .bak 与「孤儿不动」兜底。真机第 1 步验证迁移结果。
- **字幕跟随 rename/move**：这是去掉 subtitle_path 后的必要联动，若漏做则改名/移动后字幕「丢失」（其实是路径对不上）。纳入本轮，真机 4/5 步验证。
- **`ALTER TABLE DROP COLUMN` 需 SQLite ≥ 3.35**：rusqlite 捆绑的 SQLite 版本需确认支持（现代版本均支持）；若不支持，退化为「建新表迁移数据」——实现计划里以 DROP COLUMN 为主、失败则重建表。
- **category_path 含分类名与否**：确认字幕路径用「category + category_path（不含分类名）」拼，与视频推导一致，迁移和运行推导用同一函数保证对齐。
