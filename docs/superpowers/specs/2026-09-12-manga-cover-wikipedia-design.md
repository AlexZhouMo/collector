# 漫画封面源改用 中文维基百科 + weserv 代理 设计文档

**日期**：2026-09-12
**状态**：已确认

## 背景与动机

漫画封面拉取先后尝试 AniList（官方停用）、Bangumi（本机网络对 bgm.tv/api.bgm.tv/lain.bgm.tv 全域定向阻断，实测 IPv6 reset、IPv4 超时）。进一步排查发现：用户所在网络能直连的国际库（Kitsu/Google Books/Jikan）对**中文漫画标题几乎零命中**；能命中中文的（Bangumi/豆瓣）要么被封、要么图床反爬（豆瓣 418）。

**实测找到可行链路**（全部走用户可直连域名）：
- **中文维基百科 REST summary API** 按中文标题查条目、取封面 URL：`https://zh.wikipedia.org/api/rest_v1/page/summary/<标题>`。zh.wikipedia.org 用户可直连；中文命中率极高（实测 10/10：圣斗士星矢/浪客剑心/黑执事/游戏王/无限之住人/海贼王/火影忍者/剑风传奇/三国志/天生妙手 全部返回封面）。
- 封面图在 `upload.wikimedia.org`，用户直连被封（实测 100% 超时，含 Special:FilePath 302 目标）。改经 **weserv 公开图片代理**下载：`https://images.weserv.nl/?url=<完整 encoded 图片 URL>`。weserv 用户可直连；实测 10/10 均 200 成功下载。

## 决策（已确认）

- **换源为 中文维基百科（元数据/封面 URL）+ weserv（图片下载代理）**，替换 Bangumi。
- **下载策略**：直接走 weserv 代理。实测 upload.wikimedia 在该网络 100% 超时，"直连优先"只会让每本先白等一次超时（~115 本累计十几分钟）再回退，无收益——故不做直连优先，直接经 weserv。
- **匹配策略**：全名优先；summary 无结果（404/无 thumbnail）时切子系列后缀（复用 `strip_suffix_for_search`）再查一次；仍无则计未命中。
- **覆盖策略**：遍历 comic 表全部记录并覆盖 cover_path（沿用现有覆盖式，用户已确认）。
- 手动上传/裁剪封面（编辑抽屉，已实现）作为兜底保留。
- 已知噪声（不处理）：维基消歧义可能命中非漫画条目（如"三国志"命中历史书、"游戏王"命中 logo）；这类偏差用手动上传兜底，不影响主流程。

## 架构（纯后端，前端仅文案）

### 新增 `src-tauri/src/poster/wikicover.rs`

取代 bangumi.rs 的角色（bangumi.rs 删除）。函数：

```rust
/// 从 zh.wikipedia REST summary 响应提取封面原图 URL：
/// originalimage.source 优先，回退 thumbnail.source。无则 None。
pub fn parse_cover_url(json: &serde_json::Value) -> Option<String>

/// 复用现有子系列后缀切分（从 bangumi 迁移过来）：
/// 按 '.' 优先、再按空格切，先 trim；例 "战国.一统记"→"战国"。
pub fn strip_suffix_for_search(name: &str) -> String

/// 把 upload.wikimedia 图片 URL 包装成 weserv 代理下载 URL：
/// https://images.weserv.nl/?url=<url 整体百分号编码>
pub fn to_proxy_url(image_url: &str) -> String

/// 按名查 zh.wikipedia 条目封面 URL（未经代理，原始 upload.wikimedia URL）。
/// 全名查；无结果且切后缀后 != 原名且非空 → 用主名再查一次。
pub fn search_cover(name: &str) -> AppResult<Option<String>>

/// 经 weserv 代理下载封面字节。
pub fn download(url: &str) -> AppResult<Vec<u8>>
```

**请求细节**：
- 搜索：GET `https://zh.wikipedia.org/api/rest_v1/page/summary/{keyword}`（keyword 路径段需百分号编码），带 `User-Agent: zhoumo/collector`、`Accept: application/json`。404（无此条目）视为无结果返回 Ok(None)，不算错误。
- 提取：`originalimage.source` 优先，回退 `thumbnail.source`。
- 下载：`download(url)` 内部先 `to_proxy_url(url)` 包成 weserv URL，再 GET 下载字节（带 UA）。
- 用现有 ureq 2.x 用法（AgentBuilder + call/into_string/into_reader），与被删的 bangumi.rs 一致。

**summary 响应结构**：`{"title":..,"originalimage":{"source":"https://upload.wikimedia.org/..."},"thumbnail":{"source":"..."}}`（无图条目无这两个字段 → None）。

### `src-tauri/src/poster/mod.rs`
- 删 `pub mod bangumi;`，新增 `pub mod wikicover;`。

### 删除 `src-tauri/src/poster/bangumi.rs`
整文件删除（strip_suffix_for_search 迁入 wikicover.rs）。

### 修改 `src-tauri/src/lib.rs` — `fetch_manga_covers`
- `poster::bangumi::` → `poster::wikicover::`。
- 错误文案：search_cover Err → "网络错误，请检查网络或稍后重试"；返回 None → "维基百科未找到匹配漫画，可手动上传封面"；download Err → "封面下载失败，请稍后重试"。
- 失败项 suggest_note → "维基百科未命中，请手动查证或用编辑封面手动上传"。
- 其余（覆盖式遍历、to_cover、save_cover("manga_")、appdata_to_relative、update_cover_path("comic")、emit "manga-cover-progress"、结尾补帧、250ms sleep）不变。

### 修改 `src/views/NormalizeView.ts` — 文案
- 界面副标题与注释里的 "Bangumi" → "维基百科"。

## 数据流

「漫画封面拉取」按钮（NormalizeView，UI 不变）
→ `fetch_manga_covers`（遍历 comic 全部）
→ 每条 `wikicover::search_cover`（zh.wikipedia summary，全名→回退切后缀→取 originalimage/thumbnail）
→ `wikicover::download`（to_proxy_url 包 weserv → 下载）
→ `to_cover` → `save_cover(covers/comic,"manga_")` → `UPDATE comic.cover_path`
→ 逐条发 `manga-cover-progress`
→ 返回 `{ok, failed:[{title,reason}]}`。

## 错误处理

- 单条失败隔离，不中断批量。
- search_cover：404/无 thumbnail → Ok(None)（未命中，非错误）；其它 HTTP/网络错误 → Err。
- download（weserv）失败 → Err → 文案"封面下载失败，请稍后重试"。
- 前端未命中列表原样展示 reason（NormalizeView 不变）。

## 测试

- `parse_cover_url`：originalimage 优先；缺 originalimage 回退 thumbnail；两者皆无 → None。
- `strip_suffix_for_search`：迁移原 bangumi 测试用例（点号/多点号/无分隔/空格/前导空格/空串）。
- `to_proxy_url`：`to_proxy_url("https://upload.wikimedia.org/wikipedia/zh/5/54/x.jpg")` == `"https://images.weserv.nl/?url=https%3A%2F%2Fupload.wikimedia.org%2Fwikipedia%2Fzh%2F5%2F54%2Fx.jpg"`（整体百分号编码）。
- 搜索/下载需真实网络，不写单测。

## 影响与风险

- 后端：新增 wikicover.rs、删 bangumi.rs、改 mod.rs、改 lib.rs fetch_manga_covers。前端仅文案。
- 依赖第三方 weserv 图片代理（仅传输公开图片，无隐私数据）；若 weserv 不可用则下载失败，届时靠手动上传兜底。
- 维基消歧义噪声（命中非漫画条目）用手动上传兜底。
- 覆盖式：重复拉取会覆盖手动封面（已确认接受）。
- **YAGNI**：不做直连优先/回退（实测直连必超时）、不做多源、不做维基搜索消歧义打分（未命中/误命中走手动兜底）。
