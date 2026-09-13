# 漫画封面源改用 中文维基百科+weserv 实现计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 把漫画封面源从被网络阻断的 Bangumi 换成 中文维基百科（取封面 URL）+ weserv 图片代理（下载），覆盖式全量拉取，删除 bangumi.rs。

**Architecture:** 新建 `poster/wikicover.rs`（search_cover 走 zh.wikipedia REST summary、parse_cover_url、strip_suffix_for_search、to_proxy_url 包 weserv、download 经 weserv）；`poster/mod.rs` 用 wikicover 替 bangumi；删 `poster/bangumi.rs`；`lib.rs` fetch_manga_covers 改调 wikicover + 文案；`NormalizeView.ts` 文案改维基百科。用已有 urlencoding crate 编码。纯后端 + 前端文案。

**Tech Stack:** Rust（ureq 2.x / serde_json / urlencoding / image）、Tauri、vanilla-ts。

---

### Task 1: 新建 wikicover.rs（纯函数 + 搜索/下载）

**Files:**
- Create: `src-tauri/src/poster/wikicover.rs`
- Modify: `src-tauri/src/poster/mod.rs`

- [ ] **Step 1: 写失败测试 + 完整文件**

新建 `src-tauri/src/poster/wikicover.rs`，写入以下完整内容（纯函数 + 搜索/下载 + 测试）：

```rust
//! 中文维基百科封面客户端：按名查条目封面 URL(zh.wikipedia REST summary) → 经 weserv 代理下载。
//! upload.wikimedia 常被网络阻断，故下载统一走 images.weserv.nl 公开图片代理。

use crate::error::{AppError, AppResult};

/// 从 zh.wikipedia REST summary 响应提取封面原图 URL：
/// originalimage.source 优先，回退 thumbnail.source。无则 None。
pub fn parse_cover_url(json: &serde_json::Value) -> Option<String> {
    json.get("originalimage")
        .and_then(|v| v.get("source"))
        .and_then(|v| v.as_str())
        .or_else(|| {
            json.get("thumbnail")
                .and_then(|v| v.get("source"))
                .and_then(|v| v.as_str())
        })
        .map(|s| s.to_string())
}

/// 把带子系列后缀的标题切到主名用于回退检索：
/// 先 trim，按 '.' 优先、再按空格切，取第一段；无分隔符返回原串。
/// 例："战国.一统记"→"战国"；"圣斗士星矢.EPISODE.G"→"圣斗士星矢"。
pub fn strip_suffix_for_search(name: &str) -> String {
    let name = name.trim();
    let by_dot = name.split('.').next().unwrap_or(name);
    let by_space = by_dot.split(' ').next().unwrap_or(by_dot);
    by_space.trim().to_string()
}

/// 把 upload.wikimedia 图片 URL 包装成 weserv 代理下载 URL：
/// https://images.weserv.nl/?url=<url 整体百分号编码>
pub fn to_proxy_url(image_url: &str) -> String {
    format!("https://images.weserv.nl/?url={}", urlencoding::encode(image_url))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_original_then_thumbnail() {
        let j: serde_json::Value = serde_json::from_str(
            r#"{"originalimage":{"source":"https://u/o.jpg"},"thumbnail":{"source":"https://u/t.jpg"}}"#).unwrap();
        assert_eq!(parse_cover_url(&j).as_deref(), Some("https://u/o.jpg"));
        let j2: serde_json::Value = serde_json::from_str(
            r#"{"thumbnail":{"source":"https://u/t.jpg"}}"#).unwrap();
        assert_eq!(parse_cover_url(&j2).as_deref(), Some("https://u/t.jpg"));
        let j3: serde_json::Value = serde_json::from_str(r#"{"title":"x"}"#).unwrap();
        assert_eq!(parse_cover_url(&j3), None);
    }

    #[test]
    fn strips_series_suffix() {
        assert_eq!(strip_suffix_for_search("战国.一统记"), "战国");
        assert_eq!(strip_suffix_for_search("圣斗士星矢.EPISODE.G"), "圣斗士星矢");
        assert_eq!(strip_suffix_for_search("海贼王"), "海贼王");
        assert_eq!(strip_suffix_for_search("one piece manga"), "one");
        assert_eq!(strip_suffix_for_search(" 战国.记"), "战国");
        assert_eq!(strip_suffix_for_search(""), "");
    }

    #[test]
    fn wraps_weserv_proxy() {
        assert_eq!(
            to_proxy_url("https://upload.wikimedia.org/wikipedia/zh/5/54/x.jpg"),
            "https://images.weserv.nl/?url=https%3A%2F%2Fupload.wikimedia.org%2Fwikipedia%2Fzh%2F5%2F54%2Fx.jpg"
        );
    }
}
```

- [ ] **Step 2: 运行验证失败**

Run: `cd src-tauri && cargo test --lib wikicover`
Expected: 编译失败——`poster/mod.rs` 尚未声明 `pub mod wikicover;`，模块未挂载（0 tests filtered）。

- [ ] **Step 3: 挂载模块 + 实现 search_cover/download**

`src-tauri/src/poster/mod.rs`：新增 `pub mod wikicover;`（此步保留 `pub mod bangumi;` 不删——bangumi 仍被 lib.rs 引用，Task 2 才删；两行并存）。

在 `wikicover.rs` 的 `#[cfg(test)] mod tests` **之前**追加：

```rust
fn agent() -> ureq::Agent {
    ureq::AgentBuilder::new()
        .timeout(std::time::Duration::from_secs(15))
        .build()
}

/// 按名查 zh.wikipedia 条目封面 URL（返回原始 upload.wikimedia URL，未经代理）。
/// 全名查：命中即返回；仅当无结果(Ok(None))且切后缀后与原名不同才用主名再查一次；
/// 网络/HTTP 错误（除 404）直接向上传播，不触发回退。
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

/// 单次查询：GET zh.wikipedia REST summary，取封面 URL。
/// 条目不存在(404) → Ok(None)（未命中，非错误）。
fn search_once(keyword: &str) -> AppResult<Option<String>> {
    let url = format!(
        "https://zh.wikipedia.org/api/rest_v1/page/summary/{}",
        urlencoding::encode(keyword)
    );
    let resp = match agent()
        .get(&url)
        .set("Accept", "application/json")
        .set("User-Agent", "zhoumo/collector")
        .call()
    {
        Ok(r) => r,
        // 404 = 无此条目，视为未命中
        Err(ureq::Error::Status(404, _)) => return Ok(None),
        Err(ureq::Error::Status(code, _)) => {
            return Err(AppError::Other(format!("wiki search: HTTP {code}")));
        }
        Err(e) => return Err(AppError::Other(format!("wiki search: {e}"))),
    };
    let text = resp
        .into_string()
        .map_err(|e| AppError::Other(format!("wiki body: {e}")))?;
    let json: serde_json::Value =
        serde_json::from_str(&text).map_err(|e| AppError::Other(format!("wiki json: {e}")))?;
    Ok(parse_cover_url(&json))
}

/// 经 weserv 代理下载封面字节。
pub fn download(url: &str) -> AppResult<Vec<u8>> {
    let proxied = to_proxy_url(url);
    let resp = agent()
        .get(&proxied)
        .set("User-Agent", "zhoumo/collector")
        .call()
        .map_err(|e| AppError::Other(format!("wiki download: {e}")))?;
    let mut buf = Vec::new();
    std::io::Read::read_to_end(&mut resp.into_reader(), &mut buf)
        .map_err(|e| AppError::Other(format!("wiki read: {e}")))?;
    Ok(buf)
}
```

- [ ] **Step 4: 运行验证通过**

Run: `cd src-tauri && cargo test --lib wikicover && cargo build --lib 2>&1 | grep -E "^error"`
Expected: 3 个测试（parses_original_then_thumbnail、strips_series_suffix、wraps_weserv_proxy）PASS、无 error。

- [ ] **Step 5: 提交**

```bash
git add src-tauri/src/poster/wikicover.rs src-tauri/src/poster/mod.rs
git commit -m "feat(comic): 新增中文维基百科封面客户端(zh.wikipedia summary + weserv 代理下载)

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>"
```

---

### Task 2: fetch_manga_covers 换用 wikicover + 删 bangumi + 前端文案

**Files:**
- Modify: `src-tauri/src/lib.rs`（fetch_manga_covers）
- Modify: `src-tauri/src/poster/mod.rs`（删 `pub mod bangumi;`）
- Delete: `src-tauri/src/poster/bangumi.rs`
- Modify: `src/views/NormalizeView.ts`（文案）

- [ ] **Step 1: 改 fetch_manga_covers（`src-tauri/src/lib.rs`）**

当前 result 闭包里搜索段（注意：现有代码含 Task-前置的 429 差异化文案）为：

```rust
                let url = poster::bangumi::search_cover(&it.title)
                    .map_err(|e| {
                        // Bangumi 无 token 高频请求易触发 429，给差异化文案便于用户自查
                        if e.to_string().contains("429") {
                            "请求过于频繁（Bangumi 限流），请稍后重试".to_string()
                        } else {
                            "网络错误，请检查网络或稍后重试".to_string()
                        }
                    })?
                    .ok_or_else(|| "Bangumi 未找到匹配漫画，可手动上传封面".to_string())?;
                std::thread::sleep(std::time::Duration::from_millis(250));
                let bytes =
                    poster::bangumi::download(&url).map_err(|_e| "网络错误，请检查网络或稍后重试".to_string())?;
```

替换为（去掉 429 特判——维基/weserv 无此需求；换 wikicover；下载文案改"封面下载失败"）：

```rust
                let url = poster::wikicover::search_cover(&it.title)
                    .map_err(|_e| "网络错误，请检查网络或稍后重试".to_string())?
                    .ok_or_else(|| "维基百科未找到匹配漫画，可手动上传封面".to_string())?;
                std::thread::sleep(std::time::Duration::from_millis(250));
                let bytes =
                    poster::wikicover::download(&url).map_err(|_e| "封面下载失败，请稍后重试".to_string())?;
```

再把失败项 suggest_note：

```rust
                        suggest_note: "Bangumi 未命中，请手动查证或用编辑封面手动上传".to_string(),
```

替换为：

```rust
                        suggest_note: "维基百科未命中，请手动查证或用编辑封面手动上传".to_string(),
```

其余逻辑（覆盖式 targets、to_cover、save_cover(...,"manga_")、appdata_to_relative、update_cover_path("comic",...)、emit "manga-cover-progress"、结尾补帧、函数文档注释）保持不变。若函数文档注释中含 "Bangumi" 字样，一并改为 "维基百科" 以免残留（属该函数自身注释）。

- [ ] **Step 2: 删除 bangumi**

- `src-tauri/src/poster/mod.rs`：删除 `pub mod bangumi;` 那一行（保留 `pub mod wikicover;`）。
- 删文件：

```bash
git rm src-tauri/src/poster/bangumi.rs
```

- [ ] **Step 3: 前端文案（`src/views/NormalizeView.ts`）**

把界面副标题（漫画封面拉取卡片内）：

```
从 Bangumi 为所有漫画自动拉取封面（覆盖已有封面）
```

改为：

```
从维基百科为所有漫画自动拉取封面（覆盖已有封面）
```

把注释：

```
  // 漫画封面拉取（从 Bangumi 为所有漫画自动拉取封面，覆盖式），仿海报逻辑
```

改为：

```
  // 漫画封面拉取（从维基百科为所有漫画自动拉取封面，覆盖式），仿海报逻辑
```

- [ ] **Step 4: 验证**

先确认无残留引用：`grep -rn "bangumi\|Bangumi" src-tauri/src src` 应无输出（若有请报告，勿改无关文件）。
Run: `cd src-tauri && cargo build --lib 2>&1 | grep -E "^error"`（预期无输出）
Run: `cd src-tauri && cargo test --lib 2>&1 | tail -3`（预期全 PASS）
Run: `npx tsc --noEmit`（在项目根，预期无输出）

- [ ] **Step 5: 提交**

```bash
git add src-tauri/src/lib.rs src-tauri/src/poster/mod.rs src/views/NormalizeView.ts
git commit -m "feat(comic): 漫画封面拉取换用中文维基百科+weserv，删除被阻断的 Bangumi 代码

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>"
```

---

## 自查

- **规格覆盖**：wikicover 客户端（Task 1：parse_cover_url/strip_suffix_for_search/to_proxy_url/search_cover(404→None,切后缀回退)/download(经 weserv)，UA、zh.wikipedia summary）；fetch_manga_covers 换源 + 覆盖式保留 + 文案（Task 2）；删 bangumi（Task 2）；前端文案（Task 2）。规格各条均有对应任务。
- **占位符扫描**：无 TBD；每步给完整代码/命令。
- **类型/命名一致性**：`wikicover::search_cover/download/parse_cover_url/strip_suffix_for_search/to_proxy_url` Task 1 定义、Task 2 调用一致；`to_cover(&bytes)`/`save_cover(&covers,&cover,"manga_")`/`update_cover_path(&db,"comic",id,&cover_path)`/`appdata_to_relative` 沿用现有签名未改；urlencoding::encode 与 tmdb.rs 现有用法一致。
- **视频回归风险**：无——fetch_posters/tmdb 完全不动，仅漫画路径改动。
- **待实施确认点**：zh.wikipedia summary 404 = 无条目已按 Ok(None) 处理（回退与未命中语义正确）；weserv 接受整体百分号编码的完整 https url（实测 200，to_proxy_url 单测锁定编码格式）；沙盒 DNS 对 wikipedia/weserv 的可达性由用户真实环境验证，纯函数单测保证解析/编码/回退正确。
