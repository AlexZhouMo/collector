# 系统冗余文件统一过滤 设计文档

**日期**：2026-09-11
**状态**：已确认

## 背景与动机

项目在多处遍历磁盘目录（视频/漫画/游戏扫描、字幕批量校准、漫画打包）。这些遍历目前
都靠**扩展名白名单**（`.mkv` / `.ass` / `.zip` / 图片）筛选，`.DS_Store`（无扩展名）
与 `Thumbs.db`（`.db`，不在任何白名单里）恰好被顺带排除——功能上**当前没有 bug**。

但这种正确是"巧合式"的：一旦将来新增一个遍历点，或某个白名单放宽，系统冗余文件就会
混入结果（进入数据库、被打进 zip、被当作字幕处理）。`normalize/comic_pack.rs` 的注释
已写明"清理无关文件（如 Thumbs.db）"，说明排除这类文件本就是既定意图，只是没有落成
一处共享、显式、有测试保障的规则。

**目标**：把"排除系统自动生成的冗余文件"变成集中、可复用、有单元测试保障的规则，并在
所有目录遍历点主动接入，做**防御式加固**。

## 识别范围

按文件名匹配一份固定清单（**大小写不敏感**）：

| 名称 | 来源 |
| --- | --- |
| `.DS_Store` | macOS Finder |
| `Thumbs.db` | Windows 缩略图缓存 |
| `Desktop.ini` | Windows 文件夹配置 |
| `.Spotlight-V100` | macOS 索引 |
| `.Trashes` | macOS 废纸篓 |
| `.fseventsd` | macOS 文件系统事件 |

外加规则：文件名以 `._` 开头的一律视为冗余（macOS AppleDouble 资源派生文件）。

**不做**："排除所有以 `.` 开头的隐藏文件"——那会误伤 `.git`、`.vscode` 等用户故意
保留的隐藏目录。仅按上述固定清单 + `._` 前缀判定。

## 组件

### 新增 `src-tauri/src/util/junk.rs`

单一职责模块，导出：

```rust
/// 系统/工具自动生成的冗余文件名清单（比较时统一转小写）。
const JUNK_NAMES: &[&str] = &[
    ".ds_store",
    "thumbs.db",
    "desktop.ini",
    ".spotlight-v100",
    ".trashes",
    ".fseventsd",
];

/// 判断一个文件/目录名是否为系统冗余文件：
/// 命中 JUNK_NAMES（大小写不敏感），或以 `._` 开头（AppleDouble）。
pub fn is_system_junk(name: &str) -> bool;

/// 便捷版：取路径末段文件名判定；无末段（如根路径）返回 false。
pub fn is_system_junk_path(path: &std::path::Path) -> bool;
```

依赖：仅 `std`。无对项目其他模块的依赖，故任何模块都可安全 `use crate::util::junk`。

### 模块注册

在 `src-tauri/src/lib.rs` 顶部已有的 `mod` 声明处新增 `pub mod util;`；`util/mod.rs`
内 `pub mod junk;`。

## 接入点

所有真实遍历用户目录的位置，在拿到每个条目路径后、进入白名单判断前，先跳过冗余文件。

| 文件:函数 | 遍历方式 | 接入 |
| --- | --- | --- |
| `library/scanner.rs::scan_videos` | WalkDir | 循环体开头 `if junk::is_system_junk_path(p) { continue; }` |
| `library/scanner.rs::scan_videos_subs` | WalkDir | 同上 |
| `library/scanner.rs::scan_comics` | WalkDir | 同上 |
| `library/scanner.rs::scan_games` | read_dir | `dir.is_dir()` 判断后补 `if junk::is_system_junk_path(&dir) { continue; }` |
| `normalize/comic_pack.rs::pack_comic_dir` | read_dir | 在 `.filter(...)` 图片白名单里追加 `&& !junk::is_system_junk_path(p)`；把注释"清理无关文件"落成实际调用 |
| `normalize/mod.rs::run_subtitle_normalize` | WalkDir | `.filter(...)` 里追加排除冗余（`.ass` 白名单已足够，此处为防御式一致性） |

**不接入**：

- `player/transcode.rs` 缓存清理：只删自己生成的转码分片，不遍历用户文件。
- `comic/reader.rs::list_pages`：读的是 zip **内部**条目，已按图片扩展名过滤；可顺带
  加一道 `!is_system_junk(name)`，成本低且防御 zip 内混入的 `__MACOSX/._x`。

## 错误处理

判定函数是纯函数，无 IO、不返回错误。接入点的行为是"遇到冗余文件 `continue`/`filter`
掉"，不产生错误、不中断遍历，与现有 `filter_map(|e| e.ok())` 的容错风格一致。

## 测试

### `util/junk.rs` 单元测试

- 命中清单：`.DS_Store`、`Thumbs.db`、`Desktop.ini` → true。
- 大小写不敏感：`thumbs.DB`、`.ds_store`、`THUMBS.DB` → true。
- AppleDouble 前缀：`._星战.mkv`、`._foo` → true。
- 正常文件不误判：`01.jpg`、`星战.ass`、`poster.jpg`、`game.json`、`info.txt` → false。
- 边界：空字符串 `""` → false；`is_system_junk_path` 对无文件名路径 → false。

### 接入点回归测试

给现有扫描测试各补一个用例（放在同文件的 `#[cfg(test)]` 内）：

- `scan_videos`：目录里额外写入 `.DS_Store` 和 `Thumbs.db`，断言结果条目数不变、不含它们。
- `scan_comics`：同理，混入 `.DS_Store`，断言只扫到 zip。
- `pack_comic_dir`：已有 `Thumbs.db` 被忽略的测试；补一个 `.DS_Store` 一并被忽略的断言。
- `run_subtitle_normalize`：输入目录混入 `.DS_Store`，断言 report 数量只等于 `.ass` 数。

## 影响与风险

- **行为变化**：对当前真实数据**无可见变化**（这些文件本就被白名单排除）。价值在于把
  隐式正确变为显式、集中、有测试锚定，杜绝未来遍历点遗漏。
- **无数据库/接口变更**，无前端改动，无迁移。
- **YAGNI**：不做可配置清单、不做"隐藏文件"泛化、不扫 zip 外的 `__MACOSX` 目录（reader
  接入点顺带覆盖 zip 内即可）。
