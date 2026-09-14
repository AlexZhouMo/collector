# 无封面影视 · TMDB 网页版译名检索 — 设计

## 背景与目标

数据库 1288 部视频中 **29 部无封面**。当初它们在应用内 TMDB 海报抓取"未命中"，根因之一是**库内中文标题与 TMDB 官方译名不一致**（如库内「生死关头」，TMDB 官方译名为「007：你死我活」），加之 `api.themoviedb.org` 被墙。

目标：绕开被墙的官方 JSON API，用 **TMDB 网页版**为这 29 部搜出其在 TMDB 上的官方译名，输出「当前标题 → TMDB 新译名」对照表，并顺便拉取 TMDB 海报。产物供人工使用（不自动写回数据库）。

## 可行性（已实测验证）

- `api.themoviedb.org`：被墙（HTTP 000 超时）❌
- `www.themoviedb.org/search?query=…&language=zh-CN`：可达 ✅，返回 `/movie/<id>-<slug>` 结果链接
- 条目页 `www.themoviedb.org/movie/<id>?language=zh-CN`：跟随重定向(-L)可拿 `<title>` 与 `og:title` 的官方中文译名（实测「Live and Let Die」→「007：你死我活 (1973)」）
- `image.tmdb.org`：CDN 可达，海报可下

## 方案

单个 Python 脚本，离线运行，**不改应用代码、不写回数据库**。

### 数据流
1. 从 `~/Library/Application Support/com.zhoumo.collector/collector.sqlite` 的 `media` 表读 29 部无封面记录（category / category_path / title）。
2. 每部准备**多候选搜索词**（数组，按命中概率排序）：英文原名（用影视知识映射）→ 其他常见中文译名 → 库内原标题。
3. 逐词请求 `www.themoviedb.org/search`，解析 HTML 提取候选：`movie id`、`slug`、年份、中文标题。轮流搜直到拿到候选。
4. **年份消歧**：选与库内年份最接近(±1)者；年份对不上或多个同样接近 → 打 `存疑` 标记。
5. 命中后进条目页取**官方中文译名**（og:title 去掉尾部 `(年份)`）；从条目页解析 poster 路径，经 `image.tmdb.org/t/p/w500` 下海报到 `poster-candidates/_tmdb/`。
6. **输出对照表**（Markdown 文件 + 控制台）：`行号 | 当前标题 | TMDB译名 | 年份 | TMDB链接 | 存疑? | 海报文件`。

### 关键组件（单文件内的纯函数）
- `search_tmdb(query) -> [候选dict]`：网页搜索 + HTML 解析
- `pick(candidates, want_year) -> (best, suspicious)`：年份消歧 + 存疑判定
- `title_of(movie_id, slug) -> str`：条目页取官方中文译名
- `poster_of(movie_id) -> url|None` + 下载
- 主循环：遍历 29 部，多候选词轮流，落表

### 限流与错误处理
- 每部间隔延时（TMDB 网页版频繁请求会限流），传输失败重试（指数退避，复用既往经验）。
- 搜不到 → 列「未命中」，不阻塞其余。
- 解析失败 → 降级为只给 TMDB 搜索链接。
- 海报下载失败 → 仅记译名，不影响对照表。

## 输出
- `poster-candidates/tmdb译名对照.md`：对照表主产物
- `poster-candidates/_tmdb/<行号>_<译名>.jpg`：顺便拉取的 TMDB 海报

## 不做（YAGNI）
- 不改应用 DB（对照表供人工决定是否改 title）
- 不改应用代码
- 不追求 100% 自动化（存疑项标出交人复核）
- 不再纠缠被墙的官方 API（只走网页版）

## 验证
1. 脚本跑通，29 部各产出一行（命中/存疑/未命中三态之一）。
2. 抽查已知项：生死关头 → 「007：你死我活」正确。
3. 存疑项确实标出、未命中项确实列出。
4. 拉到的海报可正常打开、为竖版。
