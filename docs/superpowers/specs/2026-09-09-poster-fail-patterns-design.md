# 修复三类规律性海报抓取失败

## 目标

修复海报抓取中三类有规律、可自动救回的失败：剧集季目录带副标题、电影/动漫标题带「：副标题」、无方括号的裸年份前缀。改后增量重抓空缺。

## 三类根因（已用真实目录/TMDB 请求验证）

1. **剧集季目录带副标题**：目录形如 `剧集/美剧/斯巴达克斯/第1季：血与沙`。`parse_season` 正则 `^第0*(\d+)季$` 严格要求整段就是「第N季」，「第1季：血与沙」不匹配 → 当成无季层 → 取末段「第1季：血与沙」作剧名搜 → 失败。斯巴达克斯全 33 集因此全灭。
2. **电影/动漫标题带「：副标题」**：如 `[2004].指环王3：国王归来`，整体搜「指环王3：国王归来」命中 0；TMDB 标题为「指环王3」。大量续集片（加勒比海盗2、小鬼当家2、夺宝奇兵4 等）中招。
3. **裸年份前缀**：如 `2006.寂静岭`（无方括号），`clean_title` 只认 `[年份].`，裸 `2006.` 不剥离 → 搜「2006.寂静岭」失败。

无法自动救回的（不在本次范围）：纯译名不符（007「生死关头」vs TMDB「你死我活」）、真冷门作 → 保持空缺，用编辑抽屉手动上传（裁剪功能已就绪）。

## 决策汇总

| 项 | 决策 |
|----|------|
| 季副标题 | parse_season 容忍「第N季」后接副标题，仍解析出季号，剧名取上一层 |
| 裸年份 | clean_title 兼容 `^(\d{4})\.` 前缀 |
| 副标题降级 | 完整名优先，未命中再用冒号前主名搜（alt_name） |
| 重抓 | 增量重抓空缺（幂等），不全清 |

## 改动方案

### A. `poster/parse.rs` — parse_season 容忍季副标题
`parse_season` 改为匹配以「第N季」开头（后面可跟 `：副标题`）：
```rust
fn parse_season(seg: &str) -> Option<u32> {
    // 匹配 "第" + 数字 + "季"，其后可有副标题（如 第1季：血与沙）
    let rest = seg.strip_prefix('第')?;
    let digits: String = rest.chars().take_while(|c| c.is_ascii_digit() || *c == '0').collect();
    // 简化：取「第」后到「季」之间的数字
    let idx = rest.find('季')?;
    let num = &rest[..idx];
    let trimmed = num.trim_start_matches('0');
    let val = if trimmed.is_empty() { num.parse::<u32>().ok() } else { trimmed.parse::<u32>().ok() };
    val
}
```
（实现要点：定位「季」字，取「第」与「季」之间的数字解析；「季」后的副标题忽略。这样 `第1季：血与沙` → 1，`第07季` → 7，`第2季` → 2。非季目录如「血与沙」无「第」前缀 → None。）

剧名取法不变（parse_query 中末段是季则取倒数第二段）。

### B. `poster/parse.rs` — clean_title 兼容裸年份
`clean_title` 除 `[年份].` 外，识别 `^(\d{4})\.`：
```
[1993].侏罗纪公园 → ("侏罗纪公园", 1993)  （现状）
2006.寂静岭        → ("寂静岭", 2006)      （新增）
寂静岭             → ("寂静岭", None)
```
实现：先试 `[YYYY]` 前缀（现有逻辑）；不匹配再试开头 4 位数字 + `.` 前缀。

### C. 副标题降级搜索（电影/动漫）
`MediaQuery` 加 `alt_name: Option<String>`：
- clean_title 得到 name 后，若 name 含 `：` 或 `:`，`alt_name` = 冒号前主名（trim），否则 None。
- 数字保留（「加勒比海盗2：亡灵宝藏」→ alt_name「加勒比海盗2」）。
- 剧集 alt_name 恒为 None（剧集用剧名，无副标题问题）。

`src-tauri/src/lib.rs` fetch_cover：完整名优先、未命中降级：
```rust
let mut hit = search(&q.name, q.kind, q.year)?;
if hit.is_none() && q.is_anime { hit = search(&q.name, Tv, q.year)?; }  // 现有动漫 fallback
if hit.is_none() {
    if let Some(alt) = &q.alt_name {
        hit = search(alt, q.kind, q.year)?;
        if hit.is_none() && q.is_anime { hit = search(alt, Tv, q.year)?; }
    }
}
```
（顺序：完整名(kind) → 动漫fallback tv → 主名(kind) → 主名动漫fallback tv。剧集只走完整名。）

### D. 重抓
增量重抓空缺（现有幂等：只处理 cover_path 空的）。工具箱点「抓取缺失海报」。

## 测试策略

### `poster/parse.rs`
- parse_season：`第2季`→2、`第07季`→7、`第1季：血与沙`→1、`血与沙`→None。
- 剧集带季副标题：`parse_query("剧集","剧集/美剧/斯巴达克斯/第1季：血与沙","S01E01.红蟒")` → name「斯巴达克斯」、season Some(1)。
- clean_title：`[1993].侏罗纪公园`→(侏罗纪公园,1993)、`2006.寂静岭`→(寂静岭,2006)、`寂静岭`→(寂静岭,None)。
- alt_name：`指环王3：国王归来`→ alt_name Some(指环王3)；`星球大战`→ None。
- 各分支 parse 断言补 alt_name。

### 真机验证
1. 重抓 → 斯巴达克斯各集抓到剧海报；续集片（指环王3、加勒比海盗2 等）抓到；寂静岭抓到。
2. 剩余纯译名不符/冷门作仍空缺，手动上传。

## 明确不做（YAGNI）

- 不解决纯译名不符（超出规律范围，手动上传）。
- 不全清重抓。
- 不改剧集以外的季逻辑；不引入模糊匹配/打分。

## 风险

- parse_season 放宽后，若有目录名恰好「第X季xxx」但不是季（罕见），可能误判——实测目录均为规范「第N季[：副标题]」，风险低。
- 副标题降级多一次 API 调用（仅未命中时），限速 250ms 不变。
- 主名截断对「正名含冒号」的片可能不理想，但完整名优先已先试，仅未命中才降级，误伤面小。
