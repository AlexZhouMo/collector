# 漫画封面源改用 Bangumi(bgm.tv) 设计文档

**日期**：2026-09-12
**状态**：已确认

## 背景与动机

漫画封面拉取原用 AniList GraphQL（`graphql.anilist.co`）。实测确认 AniList 官方 API **长期停用**（任何请求返回 403 + "The AniList API has been temporarily disabled due to severe stability issues."）。

进一步排查发现根本问题不止于此：本项目漫画标题全部为**中文**（如"圣斗士星矢""浪客剑心""黑执事"），而 AniList / Kitsu / Jikan(MAL) 均为英文或罗马音检索库，中文标题命中率极低（实测 Kitsu："龙珠"→误命中"Dragon Pearl Boy"，"圣斗士星矢""火影忍者"无结果）。

**Bangumi(bgm.tv)** 是以中日文为主的 ACG 数据库，公开 API，中文标题原生支持，是本项目漫画封面的正确数据源。

（注：本机沙盒 DNS 将 api.bgm.tv 劫持解析到无关 IP，故沙盒内 curl 测不通；用户真实网络环境可用。）

## 决策（已确认）

- **换源为 Bangumi**，删除 AniList 代码（死代码，长期停用无保留价值）。
- **API**：新版 v0 `POST /v0/search/subjects`，带规范 User-Agent，不带 Access Token（公开搜索无需登录）。
- **匹配策略**：全名优先；结果为空时切掉子系列后缀再搜一次；取首条结果。
- **覆盖策略**：拉取时遍历 comic 表**全部**记录并覆盖 cover_path（当前 comic 表 0 封面，本次等于全量拉取；今后再点"拉取封面"会覆盖届时手动上传的封面——已知悉接受）。
- 手动上传/裁剪封面（编辑抽屉，已实现）作为兜底保留不变。

## 架构（纯后端，前端 UI 不变）

### 新增 `src-tauri/src/poster/bangumi.rs`

取代 `anilist.rs` 的角色。三个公开函数：

```rust
/// 从 Bangumi v0 搜索响应提取封面 URL：data[0].images.large，回退 common。
pub fn parse_cover_url(json: &serde_json::Value) -> Option<String>

/// 把带子系列后缀的标题切到主名用于回退检索：
/// 按 '.' 优先、再按空格切，取第一段；无分隔符返回原串。
/// 例："战国.一统记"→"战国"；"圣斗士星矢.EPISODE.G"→"圣斗士星矢"。
pub fn strip_suffix_for_search(name: &str) -> String

/// 按名搜 Bangumi 书籍(type=1)封面 URL。
/// 全名搜；结果空且切后缀后与原名不同 → 用主名再搜一次。取首条。
pub fn search_cover(name: &str) -> AppResult<Option<String>>

/// 下载封面字节（同现有 anilist::download 实现）。
pub fn download(url: &str) -> AppResult<Vec<u8>>
```

**请求细节**：
- URL：`https://api.bgm.tv/v0/search/subjects?limit=5`
- Method：POST，`Content-Type: application/json`
- Header：`User-Agent: zhoumo/collector`（Bangumi 要求标明应用来源，缺失可能被限流/拒绝）
- Body：`{"keyword": name, "filter": {"type": [1]}}`（type=1 = 书籍/漫画）
- 用现有 ureq 2.x 用法（`AgentBuilder` + `send_string` + `into_string`），与 anilist.rs 一致；4xx/5xx 走 `Err(ureq::Error::Status(code, resp))`。

**响应结构**（v0 search）：`{"data": [ { "images": {"large": "...", "common": "...", ...}, "name": "...", "name_cn": "..." }, ... ], "total": N}`。取 `data[0].images.large`，回退 `data[0].images.common`。空 `data` → None。

### `src-tauri/src/poster/mod.rs`

- 删除 `pub mod anilist;`，新增 `pub mod bangumi;`。

### 删除 `src-tauri/src/poster/anilist.rs`

整文件删除。

### 修改 `src-tauri/src/lib.rs` — `fetch_manga_covers`

- 调用 `anilist::` → `bangumi::`。
- 遍历改为 comic 表**全部**记录（去掉 `WHERE cover_path IS NULL`，覆盖式）。
- 每条：`bangumi::search_cover(title)` → 命中 `download` → `to_cover`（500×750 JPEG q85）→ `save_cover(covers/comic, "manga_")` → `UPDATE cover_path`（存相对路径 `covers/comic/manga_xxx.jpg`）。
- 发 `manga-cover-progress` 事件（`{done,total,current_title}`，不变）。
- 失败/未命中计入 `failed`，隔离不中断批量。

## 数据流

设置/工具箱「漫画封面拉取」按钮（NormalizeView，不变）
→ `fetch_manga_covers`（遍历 comic 全部）
→ 每条 `bangumi::search_cover`（全名→回退切后缀→首条）
→ `download` → `to_cover` → `save_cover(covers/comic,"manga_")` → `UPDATE comic.cover_path`
→ 逐条发 `manga-cover-progress`
→ 返回 `{ok, failed:[{title,reason}]}`。

## 错误处理

- 单条失败隔离，不中断批量。
- Bangumi 网络错误：reason = "网络错误，请检查网络或稍后重试"。
- 未命中（data 空）：reason = "Bangumi 未找到匹配漫画，可手动上传封面"。
- 前端未命中列表原样展示 reason（NormalizeView 不变）。

## 测试

- `parse_cover_url`：取 large；缺 large 回退 common；空 data 或缺 images → None。
- `strip_suffix_for_search`："战国.一统记"→"战国"；"圣斗士星矢.EPISODE.G"→"圣斗士星矢"；"海贼王"→"海贼王"（无分隔）；带空格"one piece manga"→"one"（按空格切）。
- 搜索/下载需真实网络，不写单测（与 anilist 一致）。

## 影响与风险

- 后端：新增 bangumi.rs、删 anilist.rs、改 mod.rs、改 lib.rs `fetch_manga_covers`。前端零改动。
- 覆盖式：今后重复点"拉取封面"会覆盖手动上传的封面（已确认接受）。
- 沙盒 DNS 劫持导致无法在本机联网自测搜索命中，需用户真实环境验证命中率；`parse_cover_url`/`strip_suffix_for_search` 纯函数单测保证解析与回退逻辑正确。
- **YAGNI**：不做多源兜底（Kitsu 等对中文无效）、不做 token、不做自动重试/轮询、不做手动改名重搜（未命中走手动上传兜底）。
