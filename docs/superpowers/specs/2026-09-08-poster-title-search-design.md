# 海报搜索名改用条目 title（电影/动漫每条独立真实海报）

## 目标

修复「电影/动漫同一文件夹下多个视频共用同一张海报」的问题：电影/动漫改用**每个视频条目自己的 title** 作 TMDB 搜索名（而非文件夹名），并从 title 剥离 `[年份].` 前缀、把年份作为 TMDB 搜索的 year 参数提高精度。剧集保持按季分组不变。改完全清现有海报后按新策略重抓。

## 根因（已通过真实数据定位）

现有 `poster/parse.rs` 的 `parse_query` 对电影/动漫取 `category_path` 末段（**文件夹名**）作搜索名。但用户目录里同一文件夹下常是**不同的作品**，例如：
- `动漫/日本/北斗神拳/` 下是 5 部不同剧场版：拉欧传·殉爱之章、拉欧传·激斗之章、尤莉亚传、托奇传、零·健四郎传。
- `动漫/日本/剑风传奇/` 下是 3 部不同剧场版：黄金时代篇Ⅰ/Ⅱ/Ⅲ。

这些被当成同一部「北斗神拳」「剑风传奇」去搜 → 命中同一 TMDB 条目 → 下载同一张海报 → 内容 hash 相同 → cover_path 相同。分组按 path 独立是正确的（每条确实各自搜了），问题在于**搜索名来源用了文件夹名，忽略了每条真实 title**。

title 噪声模式（实测）：清一色 `[年份].片名`，如 `[1993].侏罗纪公园`、`[2006].致工藤新一的挑战书～离别前的序章`。片名中间可能含 `.`、`：`、`～` 等，**不能按 `.` 粗暴切分**，只需剥掉开头 `[年份].` 前缀。

## 决策汇总

| 项 | 决策 |
|----|------|
| 电影/动漫搜索名 | 用每条视频自己的 title，剥离 `[年份].` 前缀 |
| 年份 | 作为 TMDB 搜索的 year 参数（movie 用 `year`，tv 用 `first_air_date_year`）提高精度 |
| 剧集 | 不变（目录结构取剧名+季，按季分组共用） |
| 分组逻辑 | 不变（season=None 用 path 独立，Task 已完成） |
| 搜不到 | 空缺，进失败清单（不变） |
| 清空 | 全清现有海报后重抓（电影/动漫搜索名全变，旧封面作废） |

## 改动方案

### 1. `poster/parse.rs`

`MediaQuery` 加 `year` 字段：
```rust
pub struct MediaQuery {
    pub name: String,
    pub kind: MediaKind,
    pub season: Option<u32>,
    pub year: Option<u32>,
}
```

新增纯函数 `clean_title`：
```rust
/// 从 title 剥离开头的「[年份].」前缀，返回 (干净片名, 年份)。
/// 保留片名中间的所有标点（. ： ～ 等）。无「[年份]」前缀则原样返回、year=None。
/// 例：[1993].侏罗纪公园 → ("侏罗纪公园", Some(1993))
///     [2006].致工藤新一的挑战书～离别前的序章 → ("致工藤新一的挑战书～离别前的序章", Some(2006))
///     侏罗纪公园（无前缀） → ("侏罗纪公园", None)
pub fn clean_title(title: &str) -> (String, Option<u32>) { ... }
```
实现逻辑：匹配开头 `[` + 4 位数字 + `]` + 可选 `.`，剥离该前缀，年份解析为 u32；不匹配则整串为 name、year=None。

`parse_query` 调整：
- 电影：`let (name, year) = clean_title(title);` → `MediaQuery { name, kind: Movie, season: None, year }`。
- 动漫：`let (name, year) = clean_title(title);` → `MediaQuery { name, kind: Tv, season: None, year }`。
- 剧集：不变（用目录结构取剧名+季），`year: None`。

### 2. `poster/tmdb.rs`

`search` 增加 year 参数：
```rust
pub fn search(name: &str, kind: MediaKind, year: Option<u32>, api_key: &str) -> AppResult<Option<TmdbHit>>
```
- 电影 endpoint `search/movie`，year 存在时追加 `&year={y}`。
- 剧集 endpoint `search/tv`，year 存在时追加 `&first_air_date_year={y}`。
- year=None 时不加年份参数。
- 其余（zh-CN、取 results[0]、UA、超时）不变。

### 3. `poster/mod.rs`

`fetch_cover` 闭包调用 search 时传 `q.year`：
```rust
let hit = poster::tmdb::search(&q.name, q.kind, q.year, &key) ...
```
分组逻辑（`group_key` season=None 用 path）**完全不变**。season_poster 回退逻辑不变。

### 4. 测试

`poster/parse.rs`：
- 新增 `clean_title` 测试：`[1993].侏罗纪公园` → `("侏罗纪公园", Some(1993))`；`[2006].致工藤新一的挑战书～离别前的序章` → 保留 `～`、year=2006；无前缀 `海贼王` → `("海贼王", None)`。
- `movie_takes_last_segment` 改为 `movie_uses_title`：验证电影用 title（如 `parse_query("电影","电影/科幻/星球大战","[1977].星球大战")` → name=`星球大战`、year=Some(1977)、Movie）。
- 动漫测试改为验证用 title：`parse_query("动漫","动漫/日本/北斗神拳","[2007].尤莉亚传")` → name=`尤莉亚传`、year=Some(2007)、Tv。
- 剧集测试（tv_normal_season 等）保持不变，补断言 year=None。

`poster/mod.rs` 测试：`MediaQuery` 构造处加 year 字段（分组测试逻辑不变，仅补字段编译通过）。

`poster/tmdb.rs`：无自动化测试，真机验证。

### 5. 清空重抓（一次性）
- `UPDATE media_item SET cover_path=NULL WHERE kind='video';`
- 删除 `<app_data>/covers/tmdb_*.jpg`。
- 重启应用，用户在工具箱点「抓取缺失海报」。

## 测试策略

- Rust 单测：parse 的 clean_title + 电影/动漫用 title + 剧集不变，全通过；mod 分组测试补 year 字段后通过。
- 真机验证：
  1. 清空后全空封面。
  2. 抓取 → 「北斗神拳」文件夹下 5 部剧场版各自命中不同 TMDB 条目、各自不同海报。
  3. 剧集同季仍共用一张。
  4. 搜不到的 title 空缺、进失败清单。

## 明确不做（YAGNI）

- 不改剧集解析（剧集按季正确，无需动）。
- 不做单集剧照 still 抓取（TMDB 单集多为截图非海报，且当前问题本质是搜索名用错，非缺分集图）。
- 不改 image_proc（图片处理不变）。
- 不新增应用内清空按钮（一次性脚本足够）。

## 风险

- title 中若有非 `[年份].` 的其他噪声（如清晰度标记 `1080P`、字幕组名），clean_title 不处理 → 可能降低命中率。实测样本均为规整的 `[年份].片名`，暂不过度设计；如重抓后发现某类噪声普遍导致失败，再迭代 clean_title。
- 用 title 搜后 API 调用次数与之前一致（本就每条独立），限速 250ms 不变。
- 冷门剧场版 TMDB 可能无收录 → 空缺，符合预期。
