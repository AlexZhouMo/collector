# 海报分组策略调整：电影/动漫每条独立、剧集按季

## 目标

调整海报抓取的分组策略，并清空现有已抓海报后按新策略重跑：

- **剧集**：保持按「剧名+季」分组，同季所有集共用一张封面（该季无海报回退整剧主海报）。
- **电影 / 动漫**：不分组，每个视频条目各自独立搜索 TMDB、各自一张封面（即使同名也分开处理）。搜不到直接空缺，不回退、不共用。
- 先清空现有全部海报（cover_path + 磁盘原图），再按新策略重抓。

## 背景与问题

现有 `src-tauri/src/poster/mod.rs` 的 `group_key` 只看 `MediaQuery.season`：
```rust
fn group_key(q: &MediaQuery) -> String {
    match q.season {
        Some(s) => format!("{}|{}", q.name, s),  // 剧集按季
        None => format!("{}|", q.name),            // 电影/动漫按片名 → 同名被合并
    }
}
```
问题：电影/动漫都是 `season=None`，于是**同名的多个电影/动漫被 `名|` 合并成一组、共用一张封面**。不符合「每个视频独立真实海报」的诉求。

现状数据（实测）：视频 1288 条，其中 tmdb 抓取 1224 条、空缺 64 条；磁盘 `covers/tmdb_*.jpg` 186 张（多集共用同季，图片数远少于条目数）。无手动导入封面（无 `cover_` 前缀），清空不会误删手动设置。

## 决策汇总

| 项 | 决策 |
|----|------|
| 剧集分组 | 按「剧名+季」，同季共用，季无海报回退整剧主海报（不变） |
| 电影/动漫分组 | 不分组，每条各自独立成组、各搜各存；同名也分开 |
| 无季剧集（无「第N季」层） | 归入 season=None 分支，每条独立（合理副作用，目录中此类极少） |
| 搜不到/无海报 | 进失败清单，cover_path 保持空缺（不变） |
| 清空范围 | 全清：清空全部视频 cover_path + 删除 covers/tmdb_*.jpg，再按新策略重抓 |

## 改动方案

### 1. 清空现有海报（一次性操作，实现阶段用脚本执行，不新增应用功能）
- 数据库：`UPDATE media_item SET cover_path=NULL WHERE kind='video';`
- 磁盘：删除 `<app_data>/covers/tmdb_*.jpg`。
- 应用数据库位置：`<app_data>/collector.sqlite`（identifier com.zhoumo.collector；开发期实测 `~/Library/Application Support/com.zhoumo.collector/`）。

### 2. 改分组逻辑（唯一代码改动：`src-tauri/src/poster/mod.rs`）

`group_key` 需要在 `season=None` 时用条目唯一标识，使每条独立成组。由于 `MediaQuery` 无 category/path 字段，改为让 `group_key` 接收条目的唯一 path 作为兜底 key：

```rust
/// 分组 key：
/// - 剧集有季（season=Some(n)）→「剧名|n」，同季合并共用一张。
/// - 电影/动漫/无季条目（season=None）→ 用条目唯一 path，使每条独立成组、各搜各存。
fn group_key(q: &MediaQuery, unique_path: &str) -> String {
    match q.season {
        Some(s) => format!("{}|{}", q.name, s),
        None => format!("path|{}", unique_path),
    }
}
```

`fetch_posters` 分组时传入 `it.path`：
```rust
groups.entry(group_key(&q, &it.path)).or_insert_with(|| (q.clone(), Vec::new())).1.push(it);
```

其余编排（needs_cover 只处理空封面、fetch_cover 搜索+季海报回退+下载+压缩+存盘、逐组失败隔离、进度回调、组内回填同一 cover_path）全部不变。因电影/动漫每条独立成组，组内只有一个成员，自然实现「各搜各存」。

### 3. 测试调整（`src-tauri/src/poster/mod.rs` 测试块）

- `groups_same_show_same_season`（剧集黑镜第1/2季合并成 2 组）→ **保持不变**（新逻辑对有季剧集行为一致）。
- `fill_same_cover_for_group_and_skip_existing`（当前用两个同名「沙丘」验证合并成一组、共用封面）→ **改为**验证两个同名「沙丘」电影**各自独立成组、各搜一次**（fetch_cover 被调用 2 次），且已有封面的「降临」仍被跳过。断言改为：搜索调用次数=2、report.ok=2、两个沙丘的 cover_path 各不相同（各搜各存）。
- `failed_group_recorded_not_filled`（失败进清单）→ 保持不变。

## 数据流（不变，仅分组变化）

```
点「抓取缺失海报」（工具箱）
  → fetch_posters：查 cover_path 空的视频
     分组：剧集有季→剧名|季（合并）；电影/动漫/无季→path|<path>（每条独立）
     逐组 parse→search→(季海报回退)→download→to_cover→save→回填组内所有成员
     搜不到/无海报→失败清单，cover_path 空缺
  → 返回 { ok, failed }
```

## 测试策略

- Rust 单测：改后的 3 个分组测试（剧集按季合并、电影同名各搜各存、失败入清单）全通过。
- 真机验证：
  1. 清空后视频面板全空封面。
  2. 点抓取 → 电影/动漫每条各自搜（进度总数≈空封面条目数，不再因合并而秒过一批同名）。
  3. 剧集同季各集封面一致；电影/动漫每条独立封面，同名不同片不再撞封面。
  4. 搜不到的条目保持空缺，进失败清单。

## 明确不做（YAGNI）

- 不改 parse.rs（解析逻辑不变，仍产出 name/kind/season）。
- 不改 tmdb.rs / image_proc.rs（网络与图片处理不变）。
- 不新增应用内「清空海报」按钮（一次性脚本足够；如日后需要可再议）。
- 电影/动漫不做同名合并（明确要求每条独立）。

## 风险

- 电影/动漫改为每条独立后，API 调用次数上升（原同名合并的现在各调一次），抓取更慢——可接受，符合诉求；限速 250ms 不变。
- 无季剧集条目变为每条独立搜——此类目录极少，且无季信息本就无法安全共用，合理。
