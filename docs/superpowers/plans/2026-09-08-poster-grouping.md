# 海报分组策略调整 实现计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 让电影/动漫每个视频各自独立抓一张真实海报（同名也分开），剧集仍按季共用；先清空现有海报再按新策略重抓。

**Architecture:** 唯一代码改动是 `src-tauri/src/poster/mod.rs` 的 `group_key`——`season=None` 时改用条目唯一 path 作 key，使每条独立成组。外加一次性清空脚本（DB cover_path 置空 + 删磁盘 tmdb 图）。

**Tech Stack:** Rust（rusqlite）；sqlite3 CLI（清空）。

**设计文档:** `docs/superpowers/specs/2026-09-08-poster-grouping-design.md`

**既有形态:**
- `group_key(q: &MediaQuery) -> String`（`poster/mod.rs:26`），当前 `season=None` 返回 `format!("{}|", q.name)`。
- 分组调用：`groups.entry(group_key(&q)).or_insert_with(...)`（`poster/mod.rs` fetch_posters 内）。
- 测试 `mk(id,cat,cpath,title,cover)` 造的 MediaItem 其 `path = "/p/{id}"`（唯一）。
- DB：`~/Library/Application Support/com.zhoumo.collector/collector.sqlite`，表 media_item，封面目录同级 `covers/`。

---

### Task 1: 改 group_key + 分组调用传 path

**Files:**
- Modify: `src-tauri/src/poster/mod.rs`

- [ ] **Step 1: 改 group_key 签名与实现**

把 `poster/mod.rs` 中的 `group_key` 函数（含文档注释）替换为：

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

- [ ] **Step 2: 改分组调用处传入 it.path**

在 fetch_posters 中，找到分组那一行：
```rust
        groups.entry(group_key(&q)).or_insert_with(|| (q.clone(), Vec::new())).1.push(it);
```
改为：
```rust
        groups.entry(group_key(&q, &it.path)).or_insert_with(|| (q.clone(), Vec::new())).1.push(it);
```
（`it` 是 `&&MediaItem`，`&it.path` 取到 `&str`/`&String`，可用。若类型报错，用 `it.path.as_str()`。）

- [ ] **Step 3: 更新 fill_same_cover 测试为「同名各搜各存」**

把测试 `fill_same_cover_for_group_and_skip_existing` 整个替换为下述（改名为 independent 更贴切，同时保留 skip existing 验证）：

```rust
    #[test]
    fn movies_same_name_independent_and_skip_existing() {
        let db = Db::open_in_memory().unwrap();
        {
            let conn = db.0.lock().unwrap();
            for i in 1..=3i64 {
                conn.execute(
                    "INSERT INTO media_item (id,kind,category,category_path,title,path,scanned_at) VALUES (?1,'video','电影','x','t',?2,0)",
                    rusqlite::params![i, format!("/p/{i}")],
                ).unwrap();
            }
        }
        let items = vec![
            mk(1, "电影", "电影/科幻/沙丘", "沙丘", None),
            mk(2, "电影", "电影/科幻/沙丘", "沙丘", None), // 同名，但 path 不同 → 独立成组
            mk(3, "电影", "电影/科幻/降临", "降临", Some("/covers/existing.jpg")), // 已有封面→跳过
        ];
        let mut calls = 0;
        let report = fetch_posters(
            &db, &items,
            |_q| { calls += 1; Ok(format!("/covers/dune{calls}.jpg")) },
            |_d, _t, _title| {},
        ).unwrap();
        assert_eq!(calls, 2, "两个同名沙丘各自独立成组，各搜一次");
        assert_eq!(report.ok, 2, "沙丘两个视频各自回填，降临已有封面被跳过");
        // 两个沙丘各搜各存，cover_path 不同
        let conn = db.0.lock().unwrap();
        let c1: String = conn.query_row("SELECT cover_path FROM media_item WHERE id=1", [], |r| r.get(0)).unwrap();
        let c2: String = conn.query_row("SELECT cover_path FROM media_item WHERE id=2", [], |r| r.get(0)).unwrap();
        assert_ne!(c1, c2, "同名电影各自独立封面，不应相同");
    }
```

- [ ] **Step 4: 运行 poster 测试**

Run: `cd /Users/zhoumo/Documents/Claude/collector/src-tauri && cargo test poster::tests 2>&1 | tail -20`
Expected: 3 个测试全 PASS（`groups_same_show_same_season`、`movies_same_name_independent_and_skip_existing`、`failed_group_recorded_not_filled`）。

- [ ] **Step 5: 全后端回归**

Run: `cd /Users/zhoumo/Documents/Claude/collector/src-tauri && cargo test 2>&1 | tail -6`
Expected: 全部通过。

- [ ] **Step 6: Commit**

```bash
cd /Users/zhoumo/Documents/Claude/collector
git add src-tauri/src/poster/mod.rs
git commit -m "feat(poster): 电影/动漫每条独立成组（season=None 用 path 作 key），剧集仍按季"
```

---

### Task 2: 清空现有海报（一次性）

**Files:** 无（数据操作）

- [ ] **Step 1: 清空 DB cover_path + 删磁盘 tmdb 图**

先确保应用未占用 DB（下条会重启，先执行清空）。运行：
```bash
DB="/Users/zhoumo/Library/Application Support/com.zhoumo.collector/collector.sqlite"
echo "清空前 tmdb 封面数: $(sqlite3 "$DB" "SELECT count(*) FROM media_item WHERE cover_path LIKE '%tmdb_%';")"
sqlite3 "$DB" "UPDATE media_item SET cover_path=NULL WHERE kind='video';"
rm -f "$(dirname "$DB")/covers"/tmdb_*.jpg
echo "清空后有封面数: $(sqlite3 "$DB" "SELECT count(*) FROM media_item WHERE kind='video' AND cover_path IS NOT NULL AND trim(cover_path)!='';")"
echo "剩余 tmdb 图片文件: $(ls "$(dirname "$DB")/covers"/tmdb_*.jpg 2>/dev/null | wc -l)"
```
Expected: 清空后有封面数=0，剩余 tmdb 图片文件=0。

注意：若应用正在运行且锁定了 sqlite，先在下一任务重启应用前执行本步；sqlite3 CLI 写入即时生效，应用下次读表会拿到空封面。

---

### Task 3: 重启应用并真机验证

**Files:** 无（验证）

- [ ] **Step 1: 重启应用**

```bash
pkill -f "tauri dev" 2>/dev/null; pkill -f "target/debug/collector" 2>/dev/null; pkill -f "vite" 2>/dev/null; sleep 2
lsof -ti:1420 2>/dev/null | xargs kill -9 2>/dev/null; sleep 1
find /Users/zhoumo/Documents/Claude/collector/src-tauri -name ".DS_Store" -delete 2>/dev/null
cd /Users/zhoumo/Documents/Claude/collector && nohup npm run tauri dev > /tmp/collector-dev.log 2>&1 & disown
```
等待编译完成（约 40s），确认 `target/debug/collector` 进程存在。

- [ ] **Step 2: 真机验证清单（用户在应用内操作）**

1. 视频面板刷新 → 全部空封面（清空生效）。
2. 工具箱点「抓取缺失海报」→ 进度总数≈全部空封面条目数；电影/动漫每条各自搜（不再因同名合并而一次覆盖多条）。
3. 抓取完成 → 剧集同季各集封面一致；电影/动漫每条独立封面，同名不同片不再撞同一张。
4. 搜不到的条目保持空缺，出现在失败清单。

---

## Self-Review

- **Spec 覆盖**：电影/动漫每条独立✓(T1 group_key season=None 用 path)；剧集按季不变✓(T1 season=Some 分支未改)；搜不到空缺✓(现有 fetch_cover 逻辑不变)；全清重抓✓(T2/T3)。
- **占位扫描**：无 TBD；清空为一次性脚本（非占位）。
- **类型一致性**：`group_key(q, unique_path)` 新签名 T1 Step1 定义、Step2 调用一致；测试 `mk` 造的 path 唯一，保证同名电影独立成组断言成立。
- **注意**：sqlite3 CLI 清空需在应用未写占用时执行；应用读表即时生效，无需改代码支持清空。
