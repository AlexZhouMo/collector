# 动漫海报抓取修复：先 movie 后 tv fallback

## 目标

修复「动漫海报大量抓取失败」（286 条空缺，命中率 <2%）。根因是动漫用错搜索端点，非数据源缺失。改为动漫先搜 movie（剧场版），未命中再 fallback 搜 tv（TV 动画），取先命中的。

## 根因（已用真实 TMDB 请求证实）

- 实测命中率：电影 399/430（93%），动漫 5/291（<2%）。
- 空缺动漫 title 均为 TMDB 收录的知名作：幽灵公主、萤火虫之墓、超人总动员2、大雄的金银岛、福音战士新剧场版等。
- 当前 `parse_query` 把动漫一律当 `Tv`（走 `search/tv`）。但库里动漫绝大多数是**剧场版电影**（TMDB 属 movie）。
- 实测：「萤火虫之墓」tv 命中 0 / movie 命中 4；「超人总动员2」tv 0 / movie 2；「幽灵公主」tv 1 / movie 5。
- 结论：不是数据源问题，是搜索端点用错。无需替代数据源。

## 决策汇总

| 项 | 决策 |
|----|------|
| 动漫搜索 | 先 movie，未命中 fallback tv，取先命中 |
| 表达方式 | MediaQuery 加 is_anime: bool，动漫 kind=Movie + is_anime=true |
| 电影/剧集 | 不变 |
| 重抓范围 | 只重抓空缺（现有幂等逻辑），不全清；电影/剧集已抓的保留 |

## 改动方案

### 1. `poster/parse.rs`
`MediaQuery` 加字段：
```rust
pub struct MediaQuery {
    pub name: String,
    pub kind: MediaKind,
    pub season: Option<u32>,
    pub year: Option<u32>,
    pub is_anime: bool,
}
```
`parse_query` 各分支：
- 电影：`kind: Movie, is_anime: false`（同现状 + 新字段）。
- 动漫：`kind: Movie, is_anime: true`（**默认先搜 movie**，从 Tv 改为 Movie）。
- 剧集：`kind: Tv, is_anime: false`。

### 2. `src-tauri/src/lib.rs` fetch_cover 闭包
动漫两段搜索 + fallback：
```rust
let mut hit = poster::tmdb::search(&q.name, q.kind, q.year, &key)
    .map_err(|e| format!("网络错误: {e}"))?;
if hit.is_none() && q.is_anime {
    // 剧场版没命中，试 TV 动画
    hit = poster::tmdb::search(&q.name, poster::parse::MediaKind::Tv, q.year, &key)
        .map_err(|e| format!("网络错误: {e}"))?;
}
let hit = match hit { Some(h) => h, None => return Err("搜索无结果".into()) };
```
其余（季海报回退、下载、压缩、存盘）不变。动漫 season 一般 None，季海报分支不触发。

### 3. 测试
- `poster/parse.rs`：
  - 动漫测试 `anime_uses_title_with_year` 改断言 `kind=Movie, is_anime=true, year=Some(...)`。
  - 电影测试补 `is_anime=false`。
  - 剧集测试补 `is_anime=false`。
- `poster/mod.rs`：不直接构造 MediaQuery，不受影响（经 parse_query 生成）。
- tmdb.rs：无自动化测试，真机验证。

### 4. 重抓
无需全清。工具箱点「抓取缺失海报」，现有幂等逻辑只处理 cover_path 空的条目（286 动漫 + 少量其他），电影/剧集已抓的跳过。

## 测试策略

- Rust 单测：parse 各分支 kind/is_anime/year 断言全过；全后端回归通过。
- 真机验证：
  1. 工具箱「抓取缺失海报」→ 动漫命中率大幅回升（幽灵公主、萤火虫之墓等抓到）。
  2. 剧场版取 movie 海报；纯 TV 动画（若有）fallback 到 tv。
  3. 电影/剧集不受影响。
  4. 仍搜不到的冷门作空缺、进失败清单。

## 明确不做（YAGNI）

- 不引入替代数据源（TMDB 数据齐全，问题在端点）。
- 不全清重抓（幂等增量补空缺即可）。
- 不改电影/剧集搜索逻辑。
- 不做「同时搜 movie+tv 取更优」的复杂打分（先 movie 后 tv 足够）。

## 风险

- 极少数 movie/tv 同名不同作时，先取 movie 可能不是期望作——但动漫库以剧场版为主，movie 优先正确率最高。
- fallback 使未命中动漫多一次 API 调用，整体稍慢；限速 250ms 不变。
