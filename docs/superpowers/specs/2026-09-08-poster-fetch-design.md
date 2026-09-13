# 自动抓取海报设计

## 目标

设置页点一个按钮，后端轮询库中所有 `cover_path` 为空的视频，用 TMDB 按中文标题搜索、下载海报、统一压缩成 500×750 JPEG 存入应用数据目录的 `covers/`，回填 `cover_path`。剧集按季共用一张封面（该季无单独海报则回退整剧主海报）。抓不到的汇总成失败清单返回前端，供人工处理。

## 背景与关键约束（真机实测）

- 项目**无 HTTP 客户端库**（当前仅有 `tiny_http` 服务端）。抓取与下载需新增网络库。
- **豆瓣不可行**：`movie.douban.com/j/subject_suggest` 与 `www.douban.com/search` 裸请求均被 `sec.douban.com` 拦截返回 302（安全验证），无法按片名自动搜索。`search_subjects` 只能按 tag 浏览热门，不支持片名搜索。故放弃豆瓣。
- **TMDB 可行且为唯一稳定自动化方案**：
  - 主域名 `api.themoviedb.org` 连接超时（被阻断，code=000）。
  - **官方备用域名 `api.tmdb.org` 可达**（假 key 返回 401，说明 API 通）。**本方案 API 一律走 `api.tmdb.org`**。
  - 图片 CDN `image.tmdb.org` 可达（404 说明可连）。
  - 需用户注册一个免费 TMDB API Key。
- 现有依赖已含 `image = "0.25"`（用于缩放/编码）、`rusqlite`、`serde_json`。settings 表已存在，用于存 Key，无需改表结构。
- 封面统一规格：**500×750（2:3 竖版）、JPEG 质量 85、居中裁剪**。与现有 UI `.poster-img` 的 `aspect-ratio:2/3` + `object-fit:cover` 完全一致。
- 存储位置：`<app_data>/covers/`（与现有手动导入封面同目录，命名 `tmdb_<hash>.jpg`）。

## 决策汇总

| 项 | 决策 |
|----|------|
| 数据源 | TMDB（`api.tmdb.org` API + `image.tmdb.org` 图源），中文搜索 `language=zh-CN` |
| 后备源 | 无（豆瓣不可行；不接其他后备） |
| 剧集封面粒度 | 按季优先：取该季 season poster；该季无海报则回退整剧主 poster。同季所有集共用同一封面 |
| 封面规格 | 500×750 JPEG q85，居中裁剪 |
| 触发与范围 | 设置页手动按钮，只处理 `cover_path` 为空的视频（补空缺，不覆盖已有） |
| 抓取失败 | 逐项隔离，记入失败清单返回前端展示，人工处理（用右键「编辑」手动设封面） |
| 执行方式 | 后台线程 + Tauri 事件实时进度 |

## 架构与数据流

```
前端「抓取缺失海报」按钮（设置页）
  → invoke fetch_posters（后端后台线程，poster-progress 事件推进度）
     查所有 cover_path IS NULL OR '' 的视频
     按「剧+季」分组去重（电影/动漫每视频自成一组）
     对每组：
       1. parse: category/category_path/title → MediaQuery{name, kind, season}
       2. tmdb.search(name, kind, key) → TmdbHit{id, poster_path}
       3. 若 kind=Tv 且 season 有值：tmdb.season_poster(id, season, key)
          命中用季海报，否则回退步骤 2 的剧主 poster_path
       4. tmdb.download(poster_path) → 原图字节（image.tmdb.org/t/p/w500）
       5. image_proc.to_cover(bytes) → 500×750 JPEG q85 字节
          image_proc.save_cover(covers_dir, bytes) → covers/tmdb_<hash>.jpg
       6. 回填该组所有视频 cover_path = 同一路径
       任何一步失败 → 记入失败清单，继续下一组
     组间串行 + 每次 API 调用后 sleep 250ms（远低于 TMDB ~50/s 限流）
  → 返回 FetchReport{ ok: usize, failed: Vec<FailedItem{title, reason}> }
前端弹出「成功 N / 失败 M」+ 失败列表（标题 + 原因），失败清单不落库
```

## 模块拆分与文件结构

新增模块 `src-tauri/src/poster/`：

### `poster/parse.rs`（纯函数，无 IO）
- `struct MediaQuery { name: String, kind: MediaKind, season: Option<u32> }`
- `enum MediaKind { Movie, Tv }`
- `fn parse_query(category: &str, category_path: &str, title: &str) -> MediaQuery`
- 规则：
  - `category == "剧集"`：从 `category_path` 取剧名（若末段是「第N季」则取倒数第二段作剧名，否则末段为剧名且 season=None）；解析「第N季」「第0N季」→ `season = Some(N)`。
  - `category == "电影"`：`name` = `category_path` 末段（无则用 title），`kind = Movie`，`season = None`。
  - `category == "动漫"`：`name` = 末段，`kind = Tv`（动漫按剧搜命中率更高），`season = None`。

### `poster/tmdb.rs`（网络层，`ureq`）
- `struct TmdbHit { id: u64, poster_path: Option<String> }`
- `fn search(name: &str, kind: MediaKind, api_key: &str) -> AppResult<Option<TmdbHit>>`
  - GET `https://api.tmdb.org/3/search/{movie|tv}?api_key=<key>&language=zh-CN&query=<urlencoded>`，取 results[0]。
- `fn season_poster(tv_id: u64, season: u32, api_key: &str) -> AppResult<Option<String>>`
  - GET `https://api.tmdb.org/3/tv/{id}/season/{n}?api_key=<key>&language=zh-CN`，取 poster_path。
- `fn download(poster_path: &str) -> AppResult<Vec<u8>>`
  - GET `https://image.tmdb.org/t/p/w500<poster_path>`，返回字节。
- 所有请求：自定义 User-Agent，超时 10s。

### `poster/image_proc.rs`（纯函数）
- `fn to_cover(bytes: &[u8]) -> AppResult<Vec<u8>>`：解码 → 缩放并居中裁剪成 500×750 → 编码 JPEG q85。
- `fn save_cover(covers_dir: &Path, bytes: &[u8]) -> AppResult<String>`：内容 hash 命名 `tmdb_<hash>.jpg` 存盘，返回绝对路径。

### `poster/mod.rs`（编排）
- `struct FailedItem { title: String, reason: String }`
- `struct FetchReport { ok: usize, failed: Vec<FailedItem> }`
- `fn fetch_posters<F: Fn(usize, usize, &str)>(db, covers_dir, api_key, progress: F) -> AppResult<FetchReport>`
  - 查空封面视频；按「剧+季」分组；逐组 parse→search→(season_poster)→download→to_cover→save→回填组内所有视频；失败隔离入清单；每组回调 progress。
  - 为可测：搜索+下载抽象为 trait 或闭包参数，单测传假实现，不触网。

### 改动的现有文件
- `src-tauri/Cargo.toml`：加 `ureq = { version = "2", features = ["tls"] }`。
- `src-tauri/src/lib.rs`：
  - 新命令 `fetch_posters`（后台线程 + `app.emit("poster-progress", ...)`；从 settings 读 `tmdb_api_key`，缺失则返回错误）。
  - 新命令 `set_tmdb_key(key)` / `get_tmdb_key()`（读写 settings 表 `tmdb_api_key`）。
  - `run()` 的 `generate_handler![]` 挂上三个新命令。
- `src/lib/ipc.ts`：加 `fetchPosters()`、`setTmdbKey(key)`、`getTmdbKey()`。
- `src/views/SettingsView.ts`：新增「海报」卡片——TMDB Key 输入框（预填已存值）+「抓取缺失海报」按钮 + 进度文本 + 结果/失败列表；监听 `poster-progress` 事件更新进度。

## 错误处理与进度

- **异步**：`fetch_posters` 在后台线程执行；通过 Tauri 事件 `poster-progress` 推 `{ done, total, current_title }`；按钮显示「抓取中… done/total」。
- **分组去重**：按「剧+季」归组，每组只搜一次、下一张图，组内回填同一封面。进度 total 按视频数计，同组后续视频秒过。
- **逐项隔离**：单组失败不中断，记入失败清单继续。失败原因分类：`搜索无结果` / `无海报` / `网络错误` / `图片处理失败`。
- **限速**：组间串行 + 每次 API 调用后 sleep 250ms。
- **Key 缺失**：点按钮时 settings 无 `tmdb_api_key` → 直接返回错误「请先填写 TMDB API Key」，不发请求。
- **失败清单**：`FetchReport` 返回前端展示（可滚动列表：标题 + 原因），不落库。
- **幂等**：仅处理 `cover_path IS NULL OR cover_path = ''`；反复点可补齐，不重复下载或覆盖已有。

## 测试策略

遵循现有 `#[cfg(test)]` + `tempfile` 风格，重点测无网络的纯逻辑。

### `poster/parse.rs`
- 电影 `电影/科幻/星球大战` → `{name:"星球大战", Movie, None}`
- 剧集普通季 `剧集/英剧/神探夏洛克/第2季` → `{name:"神探夏洛克", Tv, Some(2)}`
- 剧集补零季 `第07季` → `season Some(7)`
- 剧集无季层 `剧集/美剧/权力的游戏` → `{name:"权力的游戏", Tv, None}`
- 动漫末段取名，kind=Tv

### `poster/image_proc.rs`
- `to_cover`：内存生成 1000×1000 测试图 → 输出解码尺寸恰为 500×750，且为 JPEG。
- 喂宽图 1000×400 → 输出仍 500×750（验证居中裁剪不拉伸）。
- `save_cover`：存入临时目录 → 文件存在、`tmdb_` 前缀 + `.jpg`、相同字节两次调用同名（hash 稳定）。

### `poster/mod.rs`
- 分组去重：同剧同季 3 视频 + 不同季 2 视频 → 归成 2 组。
- 回填：mock 一组成功 → 组内所有视频 cover_path 写同一路径。
- 用可注入的假「搜索+下载」实现，不触网。

### `poster/tmdb.rs`
- 不写自动化单测（依赖外网 + Key）。真机验证。

### 真机验证清单
1. 未填 Key 点抓取 → 提示填 Key，不发请求。
2. 填 Key 抓取 → 进度实时更新，跑完弹「成功 N / 失败 M」+ 失败列表。
3. 面板刷新后空封面视频显示真实海报，尺寸统一（2:3 不变形）。
4. 剧集同季各集封面一致；不同季不同（若 TMDB 有单季海报）。
5. 再点一次 → 已有封面跳过，只补上次失败的。

## 明确不做（YAGNI）

- 不接豆瓣及其他后备源（已证豆瓣不可行）。
- 不做自动定时轮询（仅手动触发）。
- 不做手动搜索/多候选选择 UI（命中取第一条；抓不到的走人工编辑）。
- 不覆盖已有封面（只补空缺）。
- 不引入 async 运行时（`ureq` 同步 + 线程足够）。

## 风险

- TMDB 中文命中率：冷门片/国产剧片名不规范可能搜不到 → 进失败清单人工处理，可接受。
- `api.tmdb.org` 可达性依赖当前网络环境（实测可达）；若日后被封，需换域名或代理——本设计集中在 `tmdb.rs` 一处，改动面小。
- 季海报有无因剧而异；无单季海报时回退整剧主海报，效果仍可接受。
