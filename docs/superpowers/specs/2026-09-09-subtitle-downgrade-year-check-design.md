# 修复副标题降级错配：降级后年份校验

## 目标

修复「[2015].非常人贩：重启之战」错拿「[2002].非常人贩」海报的 bug。根因是副标题降级搜索命中后未做年份校验。修复：让 tmdb::search 返回年份，降级命中后校验年份(±1)，不符视为未命中。不使用黑名单过滤。

## 根因（已用真实 TMDB 请求证实）

- 「非常人贩：重启之战」完整名搜 TMDB 命中 0（TMDB 中文名是「玩命快递」系列，不叫「非常人贩」）。
- 触发副标题降级：用冒号前主名「非常人贩」+ year=2015 搜 → TMDB 返回玩命快递系列全 4 部（year 参数在中文查询下不严格过滤），results[0] 是 2002「玩命快递」。
- fetch_cover 的降级分支**直接采用 results[0]，未校验年份** → 2015 条目错拿 2002 海报。
- 同理 [1282] 非常人贩(2002) 降级也拿 results[0]=2002，两者撞成同一张 `tmdb_32d208dc49dd21a4.jpg`。
- suggest 那边有年份校验，fetch_cover 的降级没有——这就是漏洞。

## 决策汇总

| 项 | 决策 |
|----|------|
| 修复方式 | 降级搜索命中后年份校验(±1)，不符视为未命中；不用黑名单 |
| 校验范围 | 仅副标题降级（含动漫 fallback 的降级）；完整名精确命中不校验（不误伤正常命中） |
| search 改动 | tmdb::search 返回结果附带年份 |
| 历史数据 | 扫出因降级错配的条目，清空其 cover_path 重抓 |

## 改动方案

### 1. `src-tauri/src/poster/tmdb.rs`
`TmdbHit` 加 `year: Option<u32>` 字段；`search` 提取命中结果的 release_date/first_air_date 年份填入（复用 search_detailed 的年份提取逻辑）。
```rust
pub struct TmdbHit {
    pub id: u64,
    pub poster_path: Option<String>,
    pub year: Option<u32>,
}
```
search 的 map 中：`year: date.get(0..4).and_then(|y| y.parse().ok())`。

### 2. `src-tauri/src/lib.rs` fetch_cover
副标题降级分支：搜到 hit 后校验年份，不符则置 None（视为降级未命中）。
```rust
if hit.is_none() {
    if let Some(alt) = &q.alt_name {
        hit = poster::tmdb::search(alt, q.kind, q.year, &key).map_err(...)?;
        if hit.is_none() && q.is_anime {
            hit = poster::tmdb::search(alt, MediaKind::Tv, q.year, &key).map_err(...)?;
        }
        // 降级命中年份校验：条目有年份且命中年份存在时，必须 ±1，否则视为未命中
        if let Some(h) = &hit {
            if let (Some(y), Some(hy)) = (q.year, h.year) {
                if (y as i64 - hy as i64).abs() > 1 {
                    hit = None;
                }
            }
        }
    }
}
```
完整名搜索、动漫非降级 fallback 不加校验（保持正常命中）。

### 3. 历史数据清理（一次性脚本）
扫描所有已抓电影/动漫条目，用当前规则（含降级年份校验）重新判定；若某条**当前规则下应为空缺**（降级会被年份校验拒绝）但库里有 cover_path，说明是旧错配 → 清空其 cover_path，供重抓。
- 实操：用回归脚本思路重跑，对比出「原有 cover 但当前规则搜不到」的条目，SQL 清空这些的 cover_path。

### 4. 回归脚本同步
`scripts/poster_regression.py` 的 `resolve_poster_path` 降级分支同步加年份校验，与 Rust 一致，保证回归结果准确。

## 测试策略

- `tmdb::search` 返回 year：真机验证（网络层）。
- 修复验证：真机重抓 → [1285] 重启之战不再错拿 2002 海报（变空缺/进失败清单）；[1282][1283][1284] 正确条目不受影响。
- 回归脚本同步后重跑，确认无新的成批变化。

## 明确不做（YAGNI）

- 不用黑名单/关键词过滤。
- 不对完整名精确命中加年份校验（不误伤）。
- 不改分组/其他抓取逻辑。

## 风险

- 降级年份校验后，部分原本降级错配的条目会变空缺——这是正确行为（错配的海报本就不该要），重抓后进失败清单给建议。
- TMDB 年份标注偶尔与文件年份差异>1 的正确降级条目会被拒——但降级本就是模糊搜索，宁可空缺也不错配，符合"不要错海报"的意图。
