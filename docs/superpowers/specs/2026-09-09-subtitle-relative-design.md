# 字幕路径相对化（与封面一致）

## 目标

subtitle_path 与 cover_path 存储机制一致：字幕文件拷进 `app_data/subtitles/`（内容 hash 命名 `sub_<hash>.ass`），数据库存相对路径 `subtitles/sub_<hash>.ass`，读时（list_media）拼 app_data 变绝对。存量 1288 条现为绝对（demo/subtitles/…），需迁移。

## 决策汇总

| 项 | 决策 |
|----|------|
| 相对基准 | 相对 app_data（与 cover 一致），字幕文件拷进 app_data/subtitles/ |
| 命名 | 内容 hash：sub_<hash>.ass（与封面同思路，去重、防撞名） |
| 读时拼接 | list_media 已对 subtitle_path 做 appdata_to_absolute，无需改 |
| 写入点 | media_create/update 存前 import_subtitle 拷贝+转相对 |
| 存量迁移 | 一次性脚本：读现有绝对字幕文件→hash 拷进 app_data/subtitles/→库改相对 |

## 改动方案

### 1. 后端字幕导入函数（library/subtitle.rs 新建，类比 cover.rs）
```rust
/// 把源字幕文件按内容 hash 拷进 subtitles_dir，命名 sub_<hash>.ass，返回目标绝对路径。
pub fn import_subtitle(subtitles_dir: &Path, src: &str) -> AppResult<String>
```
- 读 src 字节 → DefaultHasher 算内容 hash → 目标名 `sub_{hash:016x}.ass` → create_dir_all + copy → 返回绝对路径。
- 源不存在则返回错误（调用方决定容错）。
- library/mod.rs 加 `pub mod subtitle;`。

### 2. 写入点转相对（lib.rs media_create/media_update）
现在字幕参数是绝对（用户选的 .ass）。改为：非空且是磁盘文件时，先 import_subtitle 拷进 app_data/subtitles/ 得绝对路径，再 appdata_to_relative 转相对存。若已是相对（subtitles/ 开头）则原样。与封面对称。

### 3. 读时拼接（已实现，无需改）
list_media 已对 subtitle_path 做 appdata_to_absolute（相对→绝对拼 app_data）。字幕存相对后自动拼回绝对，播放器/字幕加载正常。

### 4. 存量迁移（一次性脚本，类比路径迁移）
- app_data = ~/Library/Application Support/com.zhoumo.collector；subtitles_dir = app_data/subtitles。
- 遍历 media_item 有 subtitle_path 且为绝对的行：
  - 读该字幕文件字节；存在则 hash 拷进 subtitles_dir（sub_<hash>.ass），库改为相对 `subtitles/sub_<hash>.ass`。
  - 文件不存在则该条 subtitle_path 保持原值（或置空——本次保留原绝对值，避免误清；日志提示）。
- 执行前备份数据库。

## 测试策略

- import_subtitle 单测（Rust）：临时目录 + 构造 .ass 字节 → 返回路径含 subtitles + sub_ 前缀 + .ass；相同内容两次同名（hash 稳定）；源不存在报错。
- 真机：编辑视频选字幕保存 → 库存相对 subtitles/sub_xxx.ass；迁移后字幕文件在 app_data/subtitles/、库相对；播放时字幕正常。

## 明确不做（YAGNI）

- 不改 list_media 读时拼接（已支持）。
- 不改视频路径存储（本次只字幕）。
- 迁移不自动置空缺失字幕（保留原值+日志）。

## 风险

- 字幕源文件（demo/subtitles/…）须可读才能拷贝+hash；缺失的条目保留原绝对值不迁移。
- 迁移把字幕归入 app_data 管理，与视频源解耦（符合相对化方向）。
- 迁移不可逆（改库）——执行前备份。
