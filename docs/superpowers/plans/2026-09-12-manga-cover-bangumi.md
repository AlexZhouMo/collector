# 漫画封面源改用 Bangumi(bgm.tv) 实现计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 把漫画封面数据源从停用的 AniList 换成 Bangumi(bgm.tv)，支持中文标题检索，覆盖式全量拉取，删除 AniList 死代码。

**Architecture:** 新建 `poster/bangumi.rs`（search_cover/parse_cover_url/strip_suffix_for_search/download）；`poster/mod.rs` 用 bangumi 替换 anilist；删 `poster/anilist.rs`；`lib.rs` fetch_manga_covers 改调 bangumi、遍历 comic 全部记录（覆盖式）。纯后端，前端零改动。

**Tech Stack:** Rust（ureq 2.x / serde_json / image）、Tauri。

---

### Task 1: 新建 bangumi.rs（纯函数 + 搜索/下载）

**Files:**
- Create: `src-tauri/src/poster/bangumi.rs`
- Modify: `src-tauri/src/poster/mod.rs`

- [ ] **Step 1: 写失败测试**

新建 `src-tauri/src/poster/bangumi.rs`，先只放测试和空的函数签名占位不行——按 TDD 先写测试，函数未定义会编译失败。写入完整文件骨架 + 测试：

```rust
//! Bangumi(bgm.tv) v0 客户端：按名搜书籍(漫画)封面 → 下载。无需 API key。

use crate::error::{AppError, AppResult};

/// 从 Bangumi v0 搜索响应提取封面 URL：data[0].images.large，回退 common。
pub fn parse_cover_url(json: &serde_json::Value) -> Option<String> {
    let imgs = json.get("data")?.get(0)?.get("images")?;
    imgs.get("large")
        .and_then(|v| v.as_str())
        .or_else(|| imgs.get("common").and_then(|v| v.as_str()))
        .map(|s| s.to_string())
}

/// 把带子系列后缀的标题切到主名用于回退检索：
/// 按 '.' 优先、再按空格切，取第一段；无分隔符返回原串。
/// 例："战国.一统记"→"战国"；"圣斗士星矢.EPISODE.G"→"圣斗士星矢"。
pub fn strip_suffix_for_search(name: &str) -> String {
    let by_dot = name.split('.').next().unwrap_or(name);
    let by_space = by_dot.split(' ').next().unwrap_or(by_dot);
    by_space.trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_large_then_common() {
        let j: serde_json::Value = serde_json::from_str(
            r#"{"data":[{"images":{"large":"https://x/l.jpg","common":"https://x/c.jpg"}}]}"#).unwrap();
        assert_eq!(parse_cover_url(&j).as_deref(), Some("https://x/l.jpg"));
        let j2: serde_json::Value = serde_json::from_str(
            r#"{"data":[{"images":{"common":"https://x/c.jpg"}}]}"#).unwrap();
        assert_eq!(parse_cover_url(&j2).as_deref(), Some("https://x/c.jpg"));
        let j3: serde_json::Value = serde_json::from_str(r#"{"data":[]}"#).unwrap();
        assert_eq!(parse_cover_url(&j3), None);
        let j4: serde_json::Value = serde_json::from_str(r#"{"data":[{"name":"x"}]}"#).unwrap();
        assert_eq!(parse_cover_url(&j4), None);
    }

    #[test]
    fn strips_series_suffix() {
        assert_eq!(strip_suffix_for_search("战国.一统记"), "战国");
        assert_eq!(strip_suffix_for_search("圣斗士星矢.EPISODE.G"), "圣斗士星矢");
        assert_eq!(strip_suffix_for_search("海贼王"), "海贼王");
        assert_eq!(strip_suffix_for_search("one piece manga"), "one");
    }
}
```

- [ ] **Step 2: 运行验证失败**

Run: `cd src-tauri && cargo test --lib bangumi`
Expected: 编译失败——`poster/mod.rs` 尚未声明 `pub mod bangumi;`，模块未挂载。

- [ ] **Step 3: 挂载模块 + 实现 search_cover/download**

`src-tauri/src/poster/mod.rs`：把 `pub mod anilist;` 改为 `pub mod bangumi;`（anilist 在 Task 2 删除，本步先加 bangumi；此时 anilist 仍被 lib.rs 引用，保留 `pub mod anilist;` 不删——即两行并存：`pub mod anilist;` 和 `pub mod bangumi;`）。

在 `bangumi.rs` 末尾（tests 之前）补充 agent/search_cover/download：

```rust
fn agent() -> ureq::Agent {
    ureq::AgentBuilder::new()
        .timeout(std::time::Duration::from_secs(15))
        .build()
}

/// 按名搜 Bangumi 书籍(type=1)封面 URL。无需 API key。
/// 全名搜；结果空且切后缀后与原名不同 → 用主名再搜一次。取首条 data 的封面。
pub fn search_cover(name: &str) -> AppResult<Option<String>> {
    if let Some(url) = search_once(name)? {
        return Ok(Some(url));
    }
    let stripped = strip_suffix_for_search(name);
    if stripped != name && !stripped.is_empty() {
        return search_once(&stripped);
    }
    Ok(None)
}

/// 单次搜索：POST v0/search/subjects，返回首条封面 URL（无结果 → None）。
fn search_once(keyword: &str) -> AppResult<Option<String>> {
    let body = serde_json::json!({ "keyword": keyword, "filter": { "type": [1] } });
    let body_str = body.to_string();
    let resp = match agent()
        .post("https://api.bgm.tv/v0/search/subjects?limit=5")
        .set("Content-Type", "application/json")
        .set("Accept", "application/json")
        .set("User-Agent", "zhoumo/collector")
        .send_string(&body_str)
    {
        Ok(r) => r,
        Err(ureq::Error::Status(code, _resp)) => {
            return Err(AppError::Other(format!("bangumi search: HTTP {code}")));
        }
        Err(e) => return Err(AppError::Other(format!("bangumi search: {e}"))),
    };
    let text = resp
        .into_string()
        .map_err(|e| AppError::Other(format!("bangumi body: {e}")))?;
    let json: serde_json::Value =
        serde_json::from_str(&text).map_err(|e| AppError::Other(format!("bangumi json: {e}")))?;
    Ok(parse_cover_url(&json))
}

/// 下载封面字节。
pub fn download(url: &str) -> AppResult<Vec<u8>> {
    let resp = agent()
        .get(url)
        .set("User-Agent", "zhoumo/collector")
        .call()
        .map_err(|e| AppError::Other(format!("bangumi download: {e}")))?;
    let mut buf = Vec::new();
    std::io::Read::read_to_end(&mut resp.into_reader(), &mut buf)
        .map_err(|e| AppError::Other(format!("bangumi read: {e}")))?;
    Ok(buf)
}
```

- [ ] **Step 4: 运行验证通过**

Run: `cd src-tauri && cargo test --lib bangumi && cargo build --lib 2>&1 | grep -E "^error"`
Expected: 4 个测试（parses_large_then_common、strips_series_suffix）PASS、无 error。

- [ ] **Step 5: 提交**

```bash
git add src-tauri/src/poster/bangumi.rs src-tauri/src/poster/mod.rs
git commit -m "feat(comic): 新增 Bangumi(bgm.tv) 封面客户端(v0 搜索+切后缀回退+下载)

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>"
```

---

### Task 2: fetch_manga_covers 换用 bangumi + 覆盖式 + 删 anilist

**Files:**
- Modify: `src-tauri/src/lib.rs`（fetch_manga_covers）
- Modify: `src-tauri/src/poster/mod.rs`（删 `pub mod anilist;`）
- Delete: `src-tauri/src/poster/anilist.rs`

- [ ] **Step 1: 改 fetch_manga_covers**

在 `src-tauri/src/lib.rs` 的 `fetch_manga_covers` 内做三处改动：

1）去掉 `filter`（覆盖式，遍历全部）。把：

```rust
        // 只处理 cover_path 为空的漫画
        let targets: Vec<&library::model::MediaItem> = items
            .iter()
            .filter(|it| {
                it.cover_path
                    .as_deref()
                    .map(|s| s.trim().is_empty())
                    .unwrap_or(true)
            })
            .collect();
        let total = targets.len();
```

改为：

```rust
        // 覆盖式：遍历全部漫画，重新拉取并覆盖 cover_path
        let targets: Vec<&library::model::MediaItem> = items.iter().collect();
        let total = targets.len();
```

2）搜索/下载换成 bangumi，错误文案改 Bangumi。把 `result` 闭包体：

```rust
            let result: Result<String, String> = (|| {
                let url = poster::anilist::search_cover(&it.title)
                    .map_err(|e| {
                        let s = e.to_string();
                        if s.contains("暂时不可用") {
                            "AniList 服务暂时不可用，请稍后重试或手动上传封面".to_string()
                        } else {
                            format!("网络错误: {e}")
                        }
                    })?
                    .ok_or_else(|| "搜索无结果".to_string())?;
                std::thread::sleep(std::time::Duration::from_millis(250));
                let bytes =
                    poster::anilist::download(&url).map_err(|e| format!("网络错误: {e}"))?;
```

改为：

```rust
            let result: Result<String, String> = (|| {
                let url = poster::bangumi::search_cover(&it.title)
                    .map_err(|_e| "网络错误，请检查网络或稍后重试".to_string())?
                    .ok_or_else(|| "Bangumi 未找到匹配漫画，可手动上传封面".to_string())?;
                std::thread::sleep(std::time::Duration::from_millis(250));
                let bytes =
                    poster::bangumi::download(&url).map_err(|_e| "网络错误，请检查网络或稍后重试".to_string())?;
```

3）失败项的 `suggest_note` 文案改 Bangumi。把：

```rust
                        suggest_note: "AniList 未命中，请手动查证或用编辑封面手动上传".to_string(),
```

改为：

```rust
                        suggest_note: "Bangumi 未命中，请手动查证或用编辑封面手动上传".to_string(),
```

- [ ] **Step 2: 删除 anilist**

- `src-tauri/src/poster/mod.rs`：删除 `pub mod anilist;` 那一行（此时 lib.rs 已不再引用 anilist）。
- 删除文件 `src-tauri/src/poster/anilist.rs`：

```bash
git rm src-tauri/src/poster/anilist.rs
```

- [ ] **Step 3: 运行验证通过 + 全量**

Run: `cd src-tauri && cargo build --lib 2>&1 | grep -E "^error"; cargo test --lib 2>&1 | tail -3`
Expected: 无 error（无残留 `anilist::` 引用）；全部测试 PASS。

- [ ] **Step 4: 提交**

```bash
git add src-tauri/src/lib.rs src-tauri/src/poster/mod.rs
git commit -m "feat(comic): 漫画封面拉取换用 Bangumi、覆盖式全量重拉，删除停用的 AniList 代码

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>"
```

---

## 自查

- **规格覆盖**：Bangumi v0 客户端（Task 1：search_cover/parse_cover_url/strip_suffix_for_search/download，UA、type=1、切后缀回退）；fetch_manga_covers 换源 + 覆盖式（Task 2 去 filter）；删 anilist（Task 2）；错误文案区分网络错误/未命中（Task 2）；前端零改动（计划不含前端任务，符合规格）。规格各条均有对应任务。
- **占位符扫描**：无 TBD；每步给出完整代码与命令。
- **类型/命名一致性**：`bangumi::search_cover/download/parse_cover_url/strip_suffix_for_search` 在 Task 1 定义、Task 2 调用一致；`to_cover(&bytes)`/`save_cover(&covers,&cover,"manga_")`/`update_cover_path(&db,"comic",id,&cover_path)`/`appdata_to_relative(&path,&app_data)` 均沿用现有签名（未改），fetch_manga_covers 只换搜索/下载调用与文案、去 filter。
- **视频回归风险**：无——fetch_posters（视频）与 tmdb 完全不动；仅漫画路径改动。
- **待实施确认点**：Bangumi v0 响应根为 `{"data":[...]}`（已按此写 parse_cover_url）；本机沙盒 DNS 劫持无法联网自测搜索命中，纯函数单测保证解析/回退正确，真实命中率由用户环境验证；`images.large` 为完整 URL 可直接 download（实测 one piece 例证）。
