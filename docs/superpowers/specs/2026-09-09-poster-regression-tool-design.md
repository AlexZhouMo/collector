# 海报抓取回归校验工具

## 目标

一个可反复运行的独立脚本，遍历库中所有已从 TMDB 抓到海报的条目，用当前抓取规则重新搜索，比对命中的 poster_path 是否与基准一致。用于每次修改抓取规则后，确认没有把原本抓对的条目改坏（回归护栏）。

## 决策汇总

| 项 | 决策 |
|----|------|
| 形态 | 独立 Python 脚本 scripts/poster_regression.py，不改数据库、不改 Rust 抓取代码 |
| 校验粒度 | poster_path（只搜不下载不压缩，快、省流量） |
| 基准来源 | 工具旁路自建：首次 --build-baseline 跑一遍存 JSON；后续校验对比 |
| 范围 | 仅校验 cover_path 含 tmdb_ 的已抓条目 |

## 背景约束

数据库 media_item 只存最终封面路径（covers/tmdb_<hash>.jpg），不存 TMDB poster_path/id。因此「比对 poster_path」需要工具首次运行时用当前规则自建基准，无历史 poster_path 可用。

## 脚本逻辑（复刻当前 Rust 抓取规则）

脚本必须与 Rust 规则严格对应，否则比对失真。复刻：

### parse_query（对应 src-tauri/src/poster/parse.rs）
- clean_title：剥 `[YYYY].` 前缀；剥裸 `YYYY.` 前缀（前 5 字节为 4 位 ASCII 数字 + '.'）；保留中间标点。
- subtitle_main：name 含「：」或「:」→ 冒号前主名作 alt_name，否则 None。
- 电影：name=clean_title，kind=movie，is_anime=false，alt_name。
- 动漫：name=clean_title，kind=movie，is_anime=true，alt_name。
- 剧集：末段匹配「第N季[：副标题]」（定位「季」字取前面数字）→ 剧名取倒数第二段、season=N；否则末段为剧名、season=None。alt_name=None。

### 搜索顺序（对应 src-tauri/src/lib.rs fetch_cover）
1. search(name, kind, year)
2. 若未命中且 is_anime：search(name, tv, year)
3. 若仍未命中且有 alt_name：search(alt_name, kind, year)
4. 若仍未命中且 is_anime：search(alt_name, tv, year)
- search：movie 用 `&year=`，tv 用 `&first_air_date_year=`；language=zh-CN；取 results[0] 中有 poster_path 的第一条。
- 剧集命中后：若 season 有值，取 season poster_path（GET /tv/{id}/season/{n}），无则回退整剧 poster_path。
- 记录命中条目的 poster_path 作为比对值。

## 基准文件

`scripts/poster_baseline.json`：
```json
{ "<item_id>": { "title": "...", "poster_path": "/xxx.jpg" } }
```
- `--build-baseline`：遍历已抓条目，当前规则搜一遍，存基准。
- 默认（校验）：再搜一遍，与基准逐条比对。

## 比对与输出

对基准中每个 item：
- 一致：新搜 poster_path == 基准 poster_path。
- 变化：poster_path 不同 → 列 item_id/title/旧→新（回归警示）。
- 丢失：基准有、现在搜不到 → 列 item_id/title（回归警示）。
- 基准外的新条目：忽略。

终端输出：`一致 N / 变化 M / 丢失 K`；变化、丢失逐条列出。M=K=0 即规则未影响已抓结果。

## 数据来源

- DB：`~/Library/Application Support/com.zhoumo.collector/collector.sqlite`。
- Key：settings 表 tmdb_api_key。
- 限速：每请求 sleep 0.1s。
- 域名：api.tmdb.org（与 Rust 一致，主域名被墙）。

## 使用流程

1. 现在跑 `python3 scripts/poster_regression.py --build-baseline` 建立当前基准。
2. 以后每次改抓取规则后跑 `python3 scripts/poster_regression.py`，看变化/丢失是否为 0（或均为预期改善）。

## 风险

- 脚本复刻规则若与 Rust 漂移，校验失真——脚本逻辑严格对应 parse.rs/lib.rs，注释标注对应关系；这是旁路校验的固有代价。
- 基准是"当前规则的产出"，非"人工确认的正确答案"——它保障的是"规则改动前后一致"，不保障"抓得对"。抓得对与否仍靠人工看面板。

## 明确不做（YAGNI）

- 不改数据库存 poster_path。
- 不做图片 hash 级比对（poster_path 足够）。
- 不做自动修复（只报告差异）。
