# 工具箱「系统数据整理」模块（数据库重制 + 无效封面清理）

日期：2026-09-20

## Context（为什么做这件事）

工具箱（`NormalizeView`）现有海报更新、字幕校准、漫画自动归档三个维护任务。用户需要两个新的数据维护工具：整理数据库行序/ID、清理封面文件与数据库的不一致。放在「漫画自动归档」卡片之后，作为「系统数据整理」模块。

**数据现状探查**（安娜贝尔库）：三表用 `AUTOINCREMENT`（sqlite_sequence 记 media=1294/comic=115/game=135）；封面存 `covers/media/`、`covers/comic/`，相对路径 `covers/<sub>/xxx.jpg` 存库；media 有 1278 条引用但去重后 756 个不同 cover_path，且这 756 个全部有真实文件——**多条目共享封面是常态，判定必须按 DISTINCT cover_path**，否则误判。当前数据健康（文件与去重引用一一对应）。

## 已确认的决策

- 位置：工具箱「漫画自动归档」卡片之后新增「系统数据整理」卡片，两个按钮，各配新图标（database、imageClean），stroke 风格同现有、区别于 archive/trash。
- 交互：点击执行 → 展示结果（无进度条，操作快）；执行中按钮禁用显示"执行中…"。
- **数据库重制**：只事务不额外备份（依赖事务回滚兜底）；保留所有字段，纯重排序 + ID 从 1。
- **无效封面清理**：孤立图片（文件在、库不引用）**直接删**；缺失文件（库引用、文件不在）**仅提示不改库**；遍历 `covers/media` 与 `covers/comic`；判定用 DISTINCT cover_path。
- seed 释放只在 sqlite 文件不存在时触发，重制不动文件，**不会误触发 seed**（无需特殊处理）。

## 功能 1：数据库重制（后端命令 `db_reset`）

在**一个事务**内，对 media / comic / game 三表分别：
1. `SELECT *` 读出全部行（含 id 外的所有字段）。
2. 排序：
   - **media**：`category` 自定义序（电影=0 > 动漫=1 > 剧集=2 > 其他=3）→ `category_path` 升序 → `title` 升序。
   - **comic / game**（无 category 列）：`category_path` 升序 → `title` 升序。
3. `DELETE FROM <表>` + `DELETE FROM sqlite_sequence WHERE name='<表>'`（清自增计数，保证下次插入 ID 从 1）。
4. 按排序结果逐条重新插入，**保留所有原字段**（category/category_path/title/cover_path/description），让 AUTOINCREMENT 从 1 递增赋新 ID。
5. 事务提交；任一步失败自动回滚（三表全有或全无）。

**排序实现**：读出后在 Rust 内排序（自定义 category 序 + 字符串比较），或 SQL `ORDER BY CASE category WHEN '电影' THEN 0 ... END, category_path, title`。二者皆可，选可读、可测的。

**结果**：返回三表各重排条数，前端展示如"media 1278 条、comic 115 条、game 135 条，ID 已从 1 重排"。

## 功能 2：无效封面清理（后端命令 `clean_covers`）

两个方向（均以 DISTINCT cover_path 为准）：
1. **孤立图片**：遍历 `<app_data>/covers/media/`、`covers/comic/` 下每个图片文件，算出其相对路径 `covers/<sub>/<name>`，检查是否被三表任一 cover_path 引用。未被引用 → **直接删除**该文件，计数。
2. **缺失文件**：收集三表所有非空 cover_path（去重），检查 `<app_data>/<cover_path>` 文件是否存在。不存在 → 记录（哪个表/条目 title/路径），**仅提示，不改库**。

**结果**：返回 `{ deleted_orphans: N, missing: [{table,title,path}...] }`，前端展示"删除孤立图片 N 个；数据库引用但文件缺失 M 个：<列表>"。当前数据大概率报"未发现无效封面"。

**安全**：删除仅限 `covers/` 目录内文件（复用现有 `delete_cover_file` 的路径穿越防护思路，或直接在 covers 子目录内操作）。

## 改动

- **后端**：新增 `src-tauri/src/maintenance.rs`（`db_reset` + `clean_covers` 两 command + 可测的纯逻辑：排序比较器、孤立/缺失判定）；`lib.rs` 声明模块 + invoke_handler 注册。
- **前端**：`src/lib/icons.ts` 加 `database`、`imageClean` 两图标；`src/lib/ipc.ts` 加 `dbReset`/`cleanCovers` 封装；`src/views/NormalizeView.ts` 加「系统数据整理」卡片（两按钮 + 结果区 + 执行态）。

## 测试

- 后端单测（maintenance.rs）：
  - 排序比较器：media 按 电影>动漫>剧集 + path/title 升序；comic/game 按 path/title。
  - db_reset：内存 Db 建乱序几条 → reset → 验证行序 + ID 从 1 连续（含清 sqlite_sequence）。
  - 孤立/缺失判定：临时目录建 covers + 库引用，验证孤立文件识别、缺失路径识别、共享封面不误判。
- 运行验证：工具箱点两个按钮 → 结果展示正确；db_reset 后列表顺序/ID 符合预期；clean_covers 报告与实际一致。
- `cargo test`、`cargo build`、`npx tsc --noEmit` 全过。

## 现有可复用
- 工具箱卡片/结果区 UI 模式（NormalizeView 现有三任务）。
- `delete_cover_file`（lib.rs）的 covers 路径穿越防护。
- Db 事务、MediaKind、schema 的表结构。
