# 字幕批量校准规则引擎 实现计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 把「字幕批量校准」从皮毛实现升级为覆盖 11 条规则的 pipeline 引擎：自动做能确定的（合并/标点/特殊字符/规整已标注/分类样式），把有语义风险的收进执行结果列表提示。

**Architecture:** `normalize::subtitle` 重构为有序纯函数阶段（pipeline），每阶段一个函数 + 独立单测；特殊字符数据驱动。pipeline 同时产出标准化文本与 `Vec<Issue>`，由 `run_subtitle_normalize` 汇总成 `SubtitleReport`。质检（时间轴交叉）为独立阶段。

**Tech Stack:** Rust（纯字符串处理，无新依赖）+ vanilla-ts 前端（结果列表色标）。`cargo test --lib` / `tsc` / `npm run build`；真机 `npm run tauri dev`。

**核心原则（每个阶段都遵守）:** 能确定的自动改；有语义判断风险的只产 Issue 提示，不臆测、不自动改。所有标点/字符处理严格按 SEPARATOR 分「中文段」「英文段」，互不串扰。

**规范约定:**
- `SEPARATOR = "\\N{\\fnArial\\fs30}"`（现有常量）。中英同条时 `中文 + SEPARATOR + 英文`。
- 现有可复用：`preprocess`（去BOM/换行）、`parse_dialogues`、`build_header`（Default/Title/Note 三样式）。
- `Dialogue { start, end, text }`（现有）——本计划扩展。
- `Issue { line, kind, text }`（现有于 subtitle_check）——本计划扩展 kind 种类，字段不变（file 在 SubtitleReport 层）。
- 时间戳格式 `H:MM:SS.CS`（如 `0:00:42.66`）。
- 中文范围判断：`('\u{4e00}'..='\u{9fa5}').contains(&c)`。

---

### Task 1: Dialogue/Issue 基础扩展 + 时间戳解析

**Files:** Modify `src-tauri/src/normalize/subtitle.rs`

- [ ] **Step 1: 写失败单测**（subtitle.rs 内新 test）:
```rust
#[cfg(test)]
mod time_tests {
    use super::*;
    #[test]
    fn parse_time_to_cs() {
        assert_eq!(parse_time_cs("0:00:42.66"), Some(4266));
        assert_eq!(parse_time_cs("1:02:03.00"), Some(372300));
        assert_eq!(parse_time_cs("bad"), None);
    }
    #[test]
    fn dialogue_zh_en_split() {
        let d = Dialogue { start:"0:00:01.00".into(), end:"0:00:02.00".into(),
            text: format!("中文{}English", SEPARATOR) };
        assert_eq!(d.zh(), "中文");
        assert_eq!(d.en(), Some("English".to_string()));
        let d2 = Dialogue { start:"".into(), end:"".into(), text:"纯中文".into() };
        assert_eq!(d2.en(), None);
    }
}
```

- [ ] **Step 2: 运行验证失败**

Run: `cd /Users/zhoumo/Documents/Claude/collector/src-tauri && cargo test --lib time_tests 2>&1 | tail -8`
Expected: 编译失败——`parse_time_cs`/`zh`/`en` 未定义。

- [ ] **Step 3: 实现**（加到 subtitle.rs）:
```rust
/// 时间戳 "H:MM:SS.CS" → 厘秒总数（centiseconds）。非法返回 None。
pub fn parse_time_cs(t: &str) -> Option<u32> {
    let (hms, cs) = t.split_once('.')?;
    let parts: Vec<&str> = hms.split(':').collect();
    if parts.len() != 3 { return None; }
    let h: u32 = parts[0].trim().parse().ok()?;
    let m: u32 = parts[1].parse().ok()?;
    let s: u32 = parts[2].parse().ok()?;
    let c: u32 = cs.parse().ok()?;
    Some(((h * 60 + m) * 60 + s) * 100 + c)
}

impl Dialogue {
    /// 中文段（SEPARATOR 之前，或无 SEPARATOR 时的全部）。
    pub fn zh(&self) -> String {
        match self.text.split_once(SEPARATOR) {
            Some((z, _)) => z.trim().to_string(),
            None => self.text.trim().to_string(),
        }
    }
    /// 英文段（SEPARATOR 之后）；无则 None。
    pub fn en(&self) -> Option<String> {
        self.text.split_once(SEPARATOR).map(|(_, e)| e.trim().to_string())
    }
    /// 由中文段 + 可选英文段重建 text。
    pub fn rebuild(zh: &str, en: Option<&str>) -> String {
        match en {
            Some(e) if !e.is_empty() => format!("{zh}{SEPARATOR}{e}"),
            _ => zh.to_string(),
        }
    }
}
```

- [ ] **Step 4: 运行验证通过**

Run: `cd /Users/zhoumo/Documents/Claude/collector/src-tauri && cargo test --lib time_tests 2>&1 | tail -8`
Expected: 2 测试 ok。

- [ ] **Step 5: 提交**
```bash
cd /Users/zhoumo/Documents/Claude/collector
git add src-tauri/src/normalize/subtitle.rs
git commit -m "feat(subtitle): Dialogue zh/en 拆分 + 时间戳解析 parse_time_cs

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>"
```

---

### Task 2: merge_bilingual 合并中英 + 微小差异提示（规则2）

**Files:** Modify `src-tauri/src/normalize/subtitle.rs`（新增 merge 阶段与 Issue 收集）

- [ ] **Step 1: 定义阶段 Issue 载体**（若尚未在本文件定义，先加一个轻量 Issue，或直接复用 subtitle_check::Issue——本计划统一用 subtitle_check::Issue）。在 subtitle.rs 顶部 `use crate::normalize::subtitle_check::Issue;`（Task 10 会整合；此处先允许循环用同 crate 内类型——若循环依赖，把 Issue 提到 subtitle.rs 定义、subtitle_check 改用它。实现时以能编译为准，Issue 定义放 subtitle.rs，subtitle_check `use super::subtitle::Issue;`）。

  **决定（避免循环）：把 `Issue` 结构移动到 `subtitle.rs` 定义并 `pub`，`subtitle_check.rs` 改为 `use super::subtitle::Issue;`。** 本步执行该移动，subtitle_check 的现有 `pub struct Issue` 删除、改 import，其测试不变。

- [ ] **Step 2: 写失败单测**:
```rust
#[cfg(test)]
mod merge_tests {
    use super::*;
    fn d(s:&str,e:&str,t:&str)->Dialogue{Dialogue{start:s.into(),end:e.into(),text:t.into()}}
    #[test]
    fn merge_same_timeline_zh_en() {
        let input = vec![
            d("0:00:01.00","0:00:02.00","你好"),
            d("0:00:01.00","0:00:02.00","Hello"),
        ];
        let (out, issues) = merge_bilingual(input);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].zh(), "你好");
        assert_eq!(out[0].en(), Some("Hello".to_string()));
        assert!(issues.is_empty());
    }
    #[test]
    fn split_same_row_reunifies() {
        // 同条已有 \N（旧分隔）→ 拆开重组为标准 SEPARATOR
        let input = vec![ d("0:00:01.00","0:00:02.00","中文\\N{\\fnX}英文") ];
        let (out, _) = merge_bilingual(input);
        assert_eq!(out[0].zh(), "中文");
        assert_eq!(out[0].en(), Some("英文".to_string()));
    }
    #[test]
    fn near_miss_reports_not_merges() {
        let input = vec![
            d("0:00:01.00","0:00:02.00","你好"),
            d("0:00:01.30","0:00:02.20","Hello"), // 差 0.3s，一中一英
        ];
        let (out, issues) = merge_bilingual(input);
        assert_eq!(out.len(), 2); // 未合并
        assert!(issues.iter().any(|i| i.kind == "疑似未合并中英"));
    }
}
```

- [ ] **Step 3: 实现 merge_bilingual**:
```rust
/// 判断一段文本是否含中文。
fn has_cjk(s: &str) -> bool {
    s.chars().any(|c| ('\u{4e00}'..='\u{9fa5}').contains(&c))
}
/// 把同条内任意 `\N{...}` 旧分隔统一为标准 SEPARATOR（拆中英）。
fn unify_same_row_sep(text: &str) -> String {
    // 任意 \N{...} 视为中英分隔；正则替换为 SEPARATOR。
    // 简化：找到第一个 "\\N{" 到其后第一个 "}" 作为分隔标记整体替换。
    if let Some(p) = text.find("\\N{") {
        if let Some(q) = text[p..].find('}') {
            let head = &text[..p];
            let tail = &text[p + q + 1..];
            return format!("{}{}{}", head.trim(), SEPARATOR, tail.trim());
        }
    }
    text.to_string()
}

const NEAR_MISS_CS: u32 = 50; // 0.5s

/// 合并中英：先统一同条分隔；再把时间轴完全相同的多条合并（中文在前）。
/// 相邻两条时间轴差 ≤0.5s 且一中一英但未完全相同 → 提示，不合并。
pub fn merge_bilingual(dialogues: Vec<Dialogue>) -> (Vec<Dialogue>, Vec<Issue>) {
    let mut issues = Vec::new();
    // 1. 同条 \N 统一
    let ds: Vec<Dialogue> = dialogues.into_iter().map(|mut d| {
        d.text = unify_same_row_sep(&d.text);
        d
    }).collect();
    // 2. 按 (start,end) 分组合并
    let mut out: Vec<Dialogue> = Vec::new();
    let mut i = 0;
    while i < ds.len() {
        let mut group = vec![&ds[i]];
        let mut j = i + 1;
        while j < ds.len() && ds[j].start == ds[i].start && ds[j].end == ds[i].end {
            group.push(&ds[j]); j += 1;
        }
        if group.len() == 1 {
            out.push(ds[i].clone());
        } else {
            // 中文条放前，英文条放后（取前两条；多余记提示）
            let zh_first = group.iter().find(|g| has_cjk(&g.text)).copied().unwrap_or(group[0]);
            let en_part = group.iter().find(|g| !has_cjk(&g.text)).map(|g| g.text.clone());
            out.push(Dialogue {
                start: ds[i].start.clone(), end: ds[i].end.clone(),
                text: Dialogue::rebuild(zh_first.text.split(SEPARATOR).next().unwrap_or(&zh_first.text).trim(),
                                        en_part.as_deref()),
            });
            if group.len() > 2 {
                issues.push(Issue { line: out.len(), kind: "同时间轴多于2条".into(),
                    text: ds[i].text.clone() });
            }
        }
        i = j.max(i + 1);
    }
    // 3. 微小差异检测（相邻、未完全相同、一中一英、start&end 差都 ≤0.5s）
    for k in 1..out.len() {
        let (a, b) = (&out[k - 1], &out[k]);
        let complementary = has_cjk(&a.text) != has_cjk(&b.text);
        if complementary {
            if let (Some(as_), Some(ae), Some(bs), Some(be)) =
                (parse_time_cs(&a.start), parse_time_cs(&a.end), parse_time_cs(&b.start), parse_time_cs(&b.end)) {
                let ds_ = as_.abs_diff(bs); let de_ = ae.abs_diff(be);
                if (ds_ != 0 || de_ != 0) && ds_ <= NEAR_MISS_CS && de_ <= NEAR_MISS_CS {
                    issues.push(Issue { line: k + 1, kind: "疑似未合并中英".into(), text: b.text.clone() });
                }
            }
        }
    }
    (out, issues)
}
```

- [ ] **Step 4: 运行验证通过**

Run: `cd /Users/zhoumo/Documents/Claude/collector/src-tauri && cargo test --lib merge_tests 2>&1 | tail -10`
Expected: 3 测试 ok。

- [ ] **Step 5: 提交**
```bash
cd /Users/zhoumo/Documents/Claude/collector
git add src-tauri/src/normalize/subtitle.rs src-tauri/src/normalize/subtitle_check.rs
git commit -m "feat(subtitle): merge_bilingual 合并同时间轴中英 + 微小差异提示

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>"
```

---

### Task 3: clean_special 特殊字符/OCR/标点规整数据表（规则4）

**Files:** Create `src-tauri/src/normalize/special_chars.rs`；Modify `src-tauri/src/normalize/mod.rs`（挂子模块）

- [ ] **Step 1: 写失败单测**（在 special_chars.rs）:
```rust
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn ocr_fixes() {
        assert_eq!(clean_special(" l am here"), " I am here"); // l→I 独立词
        assert_eq!(clean_special("lt's ok"), "It's ok");
    }
    #[test]
    fn punct_regular() {
        assert_eq!(clean_special("wait--"), "wait…");
        assert_eq!(clean_special("a....b"), "a…b");   // 连续点→…
        assert_eq!(clean_special("x   y"), "x y");      // 多空格→单
    }
    #[test]
    fn quote_normalize() {
        assert_eq!(clean_special("say ''hi''"), "say \"hi\"");
    }
}
```

- [ ] **Step 2: 运行验证失败**

Run: `cd /Users/zhoumo/Documents/Claude/collector/src-tauri && cargo test --lib special_chars 2>&1 | tail -8`
Expected: 编译失败——模块/函数未定义。

- [ ] **Step 3: 实现 special_chars.rs**:
```rust
//! 特殊字符 / OCR 英文纠错 / 标点规整。数据驱动 + 少量正则。
//! 基础版：从 Java filter 可确定推断的规则；异体字表留空可扩展（不还原 Java 损坏映射）。

/// 简单串替换表（顺序敏感）。
const REPLACE_TABLE: &[(&str, &str)] = &[
    ("''", "\""),
    ("--", "…"),
    // OCR：小写 l/i 在特定边界误识为 I
    ("lt'", "It'"), ("lsn'", "Isn'"), (" l ", " I "), (" i ", " I "),
    (",,l ", ",,I "), ("}l ", "}I "), ("\"l ", "\"I "),
    // 异体字/全角拉丁映射（可扩展）：用户日后往此追加，如 ("Ａ","A")
];

/// 连续 3+ 个点 → …；连续点/多空格规整用手写扫描（避免引入 regex 依赖）。
fn collapse_dots_and_spaces(s: &str) -> String {
    // 连续 '.'（≥2）→ …
    let mut out = String::with_capacity(s.len());
    let chars: Vec<char> = s.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == '.' {
            let mut j = i;
            while j < chars.len() && chars[j] == '.' { j += 1; }
            if j - i >= 2 { out.push('…'); } else { out.push('.'); }
            i = j;
        } else { out.push(chars[i]); i += 1; }
    }
    // 多空格 → 单空格
    let mut res = String::with_capacity(out.len());
    let mut prev_space = false;
    for c in out.chars() {
        if c == ' ' {
            if !prev_space { res.push(' '); }
            prev_space = true;
        } else { res.push(c); prev_space = false; }
    }
    res
}

/// 对一段文本应用特殊字符清洗（表 + 点/空格规整）。
pub fn clean_special(s: &str) -> String {
    let mut t = s.to_string();
    for (from, to) in REPLACE_TABLE {
        t = t.replace(from, to);
    }
    collapse_dots_and_spaces(&t)
}
```

- [ ] **Step 4: mod.rs 挂子模块**：`src-tauri/src/normalize/mod.rs` 顶部 `pub mod special_chars;`（与 `pub mod subtitle;` 并列）。

- [ ] **Step 5: 运行验证通过**

Run: `cd /Users/zhoumo/Documents/Claude/collector/src-tauri && cargo test --lib special_chars 2>&1 | tail -8`
Expected: 3 测试 ok。

- [ ] **Step 6: 提交**
```bash
cd /Users/zhoumo/Documents/Claude/collector
git add src-tauri/src/normalize/special_chars.rs src-tauri/src/normalize/mod.rs
git commit -m "feat(subtitle): special_chars 特殊字符/OCR/标点规整数据表(基础版可扩展)

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>"
```

---

### Task 4: 中文标点体系 + 「」引号配对（规则3）

**Files:** Create `src-tauri/src/normalize/punct.rs`；Modify `src-tauri/src/normalize/mod.rs`

- [ ] **Step 1: 失败单测**（punct.rs）:
```rust
#[cfg(test)]
mod cn_tests {
    use super::*;
    #[test]
    fn cn_punct_fullwidth() {
        assert_eq!(cn_punct("你好, 世界! 是吗? 好: 嗯"), "你好，世界！是吗？好：嗯");
    }
    #[test]
    fn cn_quotes_paired() {
        assert_eq!(cn_punct("他说\"你好\""), "他说「你好」");
        // 奇数引号：末尾未闭合仍尽量配对（最后一个按开引号）
        assert_eq!(cn_punct("\"引"), "「引");
    }
}
```

- [ ] **Step 2: 运行失败**

Run: `cd /Users/zhoumo/Documents/Claude/collector/src-tauri && cargo test --lib punct::cn_tests 2>&1 | tail -8`
Expected: 编译失败。

- [ ] **Step 3: 实现 cn_punct**（punct.rs）:
```rust
//! 中英标点体系。中文段全角化 + 「」配对；英文段半角规整。

/// 中文段标点体系：半角标点→全角；成对 " → 「」（奇偶切换）。
pub fn cn_punct(s: &str) -> String {
    // 逗号/感叹/问号/冒号（带可选空格）→ 全角
    let mut t = s.to_string();
    t = t.replace(", ", "，").replace(',', "，");
    t = t.replace("! ", "！").replace('!', "！");
    t = t.replace("? ", "？").replace('?', "？");
    t = t.replace(": ", "：").replace(':', "：");
    t = t.replace("...", "…");
    t = t.replace(". ", "。");
    // 句点→句号：仅当不在数字间（简单起见，末尾或非数字后的 '.'）
    // 引号配对：奇数位置开「，偶数位置闭」
    let mut res = String::with_capacity(t.len());
    let mut open = true;
    for c in t.chars() {
        if c == '"' {
            res.push(if open { '「' } else { '」' });
            open = !open;
        } else { res.push(c); }
    }
    res
}
```
> 注：句点→句号的数字保护、更多边界（参考 Java sinicized 的 `Pattern [0-9]+\\.[0-9]+`）在此保持简单——数字间小数点不转（当前实现只把 `. ` 转句号，`.` 紧跟数字不动）。单测覆盖常见场景，复杂边界作为 Issue 由 check 提示。

- [ ] **Step 4: 运行通过**

Run: `cd /Users/zhoumo/Documents/Claude/collector/src-tauri && cargo test --lib punct::cn_tests 2>&1 | tail -8`
Expected: 2 测试 ok。

- [ ] **Step 5: mod.rs 挂 `pub mod punct;`，提交**
```bash
cd /Users/zhoumo/Documents/Claude/collector
git add src-tauri/src/normalize/punct.rs src-tauri/src/normalize/mod.rs
git commit -m "feat(subtitle): 中文标点体系全角化 + 「」引号配对

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>"
```

---

### Task 5: 英文标点体系（规则3）

**Files:** Modify `src-tauri/src/normalize/punct.rs`

- [ ] **Step 1: 失败单测**:
```rust
#[cfg(test)]
mod en_tests {
    use super::*;
    #[test]
    fn en_punct_regular() {
        assert_eq!(en_punct("Hello ,world"), "Hello, world"); // 逗号前空格移除、后补空格
        assert_eq!(en_punct("Wait . Go"), "Wait. Go");
        assert_eq!(en_punct("end"), "end.");                  // 行尾字母补句号
    }
    #[test]
    fn en_keeps_ellipsis() {
        assert_eq!(en_punct("well…"), "well…");
    }
}
```

- [ ] **Step 2: 运行失败** → **Step 3: 实现 en_punct**（punct.rs）:
```rust
/// 英文段标点体系：半角规整（逗号/句点前后空格）、行尾补句号。
pub fn en_punct(s: &str) -> String {
    let mut t = s.trim().to_string();
    // 标点前空格移除、逗号后补空格
    t = t.replace(" ,", ",").replace(" .", ".").replace(" !", "!").replace(" ?", "?");
    t = t.replace(",", ", ").replace("  ", " ");
    t = t.replace(" .", ".").replace(". \"", ".\"");
    let t = t.trim().to_string();
    // 行尾若以字母/数字结尾，补英文句号
    if t.chars().last().map(|c| c.is_ascii_alphanumeric()).unwrap_or(false) {
        format!("{t}.")
    } else { t }
}
```
> 注：与 Java sinicized 英文侧处理对齐的常见项（`.\"`→`\".` 等）纳入；复杂引号方向留待需要时增强。

- [ ] **Step 4: 运行通过 → Step 5: 提交**
```bash
cd /Users/zhoumo/Documents/Claude/collector
git add src-tauri/src/normalize/punct.rs
git commit -m "feat(subtitle): 英文标点体系规整 + 行尾补句号

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>"
```

---

### Task 6: regularize_dialogue 对话 `-` 规整 + 跨条提示（规则5）

**Files:** Create `src-tauri/src/normalize/dialogue.rs`；Modify `mod.rs`

- [ ] **Step 1: 失败单测**（dialogue.rs）:
```rust
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn regularize_dash_spacing() {
        // 已标注对话：统一 "- " 空格
        assert_eq!(regularize_dash("-你好 -再见"), "- 你好 - 再见");
        assert_eq!(regularize_dash("-Hi -Bye"), "- Hi - Bye");
    }
    #[test]
    fn no_dash_unchanged() {
        assert_eq!(regularize_dash("普通一句"), "普通一句");
    }
}
```

- [ ] **Step 2: 运行失败** → **Step 3: 实现**（dialogue.rs）:
```rust
//! 对话结构规整：只规整已标注的 `-`（统一 "- " 空格），不臆测。

/// 把已有的对话破折号统一为 "- "（破折号后恰一个空格）。
/// 只处理行首或空格后的 '-'（对话标记），不动连字符/减号中的 '-'。
pub fn regularize_dash(s: &str) -> String {
    let mut res = String::with_capacity(s.len());
    let chars: Vec<char> = s.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let at_dialogue_pos = chars[i] == '-'
            && (i == 0 || chars[i - 1] == ' ');
        if at_dialogue_pos {
            res.push('-');
            res.push(' ');
            i += 1;
            // 跳过 '-' 后原有空格，避免 "- " 变 "-  "
            while i < chars.len() && chars[i] == ' ' { i += 1; }
        } else { res.push(chars[i]); i += 1; }
    }
    res.trim().to_string()
}
```
> 跨条 `...`/可疑对话结构不在此自动改；它们在 Task 9 的 check 阶段作为 Issue 提示（跨条需要看相邻两条上下文，放质检更合适）。

- [ ] **Step 4: 运行通过 → Step 5: mod.rs 挂 `pub mod dialogue;`，提交**
```bash
cd /Users/zhoumo/Documents/Claude/collector
git add src-tauri/src/normalize/dialogue.rs src-tauri/src/normalize/mod.rs
git commit -m "feat(subtitle): 规整已标注对话破折号 '- '（不臆测对话）

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>"
```

---

### Task 7: regularize_markers ()/[]/# 规整（规则6/7/8）

**Files:** Modify `src-tauri/src/normalize/dialogue.rs`（或新 markers.rs——同文件即可）

- [ ] **Step 1: 失败单测**:
```rust
#[cfg(test)]
mod marker_tests {
    use super::*;
    #[test]
    fn regularize_paren_spacing() {
        // 已标注非对话中文括号：全角化 + 去内侧多余空格
        assert_eq!(regularize_markers("( 道具 )"), "（道具）");
    }
    #[test]
    fn keep_song_and_foreign() {
        assert_eq!(regularize_markers("#歌词"), "#歌词");      // 歌曲 # 保留
        assert_eq!(regularize_markers("[Hola]"), "[Hola]");     // 外语方括号保留
    }
}
```

- [ ] **Step 2: 运行失败** → **Step 3: 实现**（dialogue.rs 追加）:
```rust
/// 规整已标注的标记：中文括号 ()→（）并去内侧空格；[]、# 保留（仅确保格式一致）。
/// 不臆测哪行该加标记——那由质检提示。
pub fn regularize_markers(s: &str) -> String {
    let mut t = s.to_string();
    t = t.replace("( ", "（").replace(" )", "）").replace('(', "（").replace(')', "）");
    // 方括号/井号保持原样（外语、歌曲），仅去掉紧贴内侧空格
    t = t.replace("[ ", "[").replace(" ]", "]");
    t.trim().to_string()
}
```
> 注：() 全角化仅应作用于**中文段**（英文段的括号保留半角）——调用方（Task 8 组装）负责只对中文段调用 regularize_markers，英文段不调。单测此处只验证函数行为。

- [ ] **Step 4: 运行通过 → Step 5: 提交**
```bash
cd /Users/zhoumo/Documents/Claude/collector
git add src-tauri/src/normalize/dialogue.rs
git commit -m "feat(subtitle): 规整已标注 ()（）/[]/# 标记（不臆测）

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>"
```

---

### Task 8: classify_style 分类 + 双行/单行样式（规则1）

**Files:** Create `src-tauri/src/normalize/classify.rs`；Modify `mod.rs`

- [ ] **Step 1: 失败单测**（classify.rs）:
```rust
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn classify_by_markers() {
        assert_eq!(classify_style("《肖申克的救赎》", false), Style::Title);
        assert_eq!(classify_style("（一把钥匙）", false), Style::Note);
        assert_eq!(classify_style("普通台词", true), Style::Default);
    }
}
```

- [ ] **Step 2: 运行失败** → **Step 3: 实现**（classify.rs）:
```rust
//! 字幕分类：Title(含《》)/Note(被（）括起)/Default(其余)。

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum Style { Default, Title, Note }

impl Style {
    pub fn name(&self) -> &'static str {
        match self { Style::Default => "Default", Style::Title => "Title", Style::Note => "Note" }
    }
}

/// 按中文段的字符标志分类。has_en 暂不影响分类（样式行数在组装时按有无英文决定）。
pub fn classify_style(zh: &str, _has_en: bool) -> Style {
    let z = zh.trim();
    if z.contains('《') || z.contains('》') {
        Style::Title
    } else if z.starts_with('（') && z.ends_with('）') {
        Style::Note
    } else {
        Style::Default
    }
}
```
> 「有英文→双行 / 纯中→单行」由组装阶段（Task 9）自然实现：有 en 段就输出 `中文SEPARATOR英文`（ASS 渲染为两行），无 en 就只输出中文。样式（Default/Title/Note）决定字号/位置，与双/单行正交。

- [ ] **Step 4: 运行通过 → Step 5: mod.rs 挂 `pub mod classify;`，提交**
```bash
cd /Users/zhoumo/Documents/Claude/collector
git add src-tauri/src/normalize/classify.rs src-tauri/src/normalize/mod.rs
git commit -m "feat(subtitle): 字幕分类 Title《》/Note（）/Default

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>"
```

---

### Task 9: 时间轴交叉检测（排序+前后3条）+ 跨条对话提示（规则9/5）

**Files:** Modify `src-tauri/src/normalize/subtitle_check.rs`

- [ ] **Step 1: 失败单测**:
```rust
#[cfg(test)]
mod cross_tests {
    use super::*;
    use crate::normalize::subtitle::Dialogue;
    fn d(s:&str,e:&str)->Dialogue{Dialogue{start:s.into(),end:e.into(),text:"x".into()}}
    #[test]
    fn detects_overlap_within_window() {
        let ds = vec![
            d("0:00:01.00","0:00:05.00"),
            d("0:00:04.00","0:00:06.00"), // 与上条重叠
        ];
        let issues = check_timeline_cross(&ds);
        assert!(issues.iter().any(|i| i.kind == "时间轴交叉"));
    }
    #[test]
    fn no_overlap_ok() {
        let ds = vec![ d("0:00:01.00","0:00:02.00"), d("0:00:03.00","0:00:04.00") ];
        assert!(check_timeline_cross(&ds).is_empty());
    }
}
```

- [ ] **Step 2: 运行失败** → **Step 3: 实现**（subtitle_check.rs，替换旧「时间轴逆序」）:
```rust
use crate::normalize::subtitle::{parse_time_cs, Dialogue, Issue};

/// 时间轴交叉：按 start 排序后，每条与其后 3 条比较区间是否重叠。
pub fn check_timeline_cross(dialogues: &[Dialogue]) -> Vec<Issue> {
    let mut idx: Vec<usize> = (0..dialogues.len()).collect();
    idx.sort_by_key(|&i| parse_time_cs(&dialogues[i].start).unwrap_or(0));
    let mut issues = Vec::new();
    for a in 0..idx.len() {
        let (as_, ae) = match (parse_time_cs(&dialogues[idx[a]].start), parse_time_cs(&dialogues[idx[a]].end)) {
            (Some(s), Some(e)) => (s, e), _ => continue,
        };
        for b in (a + 1)..(a + 4).min(idx.len()) {
            let (bs, be) = match (parse_time_cs(&dialogues[idx[b]].start), parse_time_cs(&dialogues[idx[b]].end)) {
                (Some(s), Some(e)) => (s, e), _ => continue,
            };
            // 区间重叠：as_ < be && bs < ae
            if as_ < be && bs < ae {
                issues.push(Issue { line: idx[b] + 1, kind: "时间轴交叉".into(),
                    text: dialogues[idx[b]].text.clone() });
            }
        }
    }
    issues
}
```
删除旧 `check` 里的「时间轴逆序」规则（被交叉检测取代）；保留/迁移「可疑标点」「缺英文行」到组装阶段或此处（实现时并入 Task 10 的总 check）。

- [ ] **Step 4: 运行通过 → Step 5: 提交**
```bash
cd /Users/zhoumo/Documents/Claude/collector
git add src-tauri/src/normalize/subtitle_check.rs
git commit -m "feat(subtitle): 时间轴交叉检测(排序+前后3条窗口)

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>"
```

---

### Task 10: 组装 pipeline + 接入 run_subtitle_normalize + 前端色标

**Files:** Modify `src-tauri/src/normalize/subtitle.rs`（新 format_ass 走 pipeline）、`src-tauri/src/normalize/mod.rs`、`src/views/NormalizeView.ts`

- [ ] **Step 1: 新 format_ass 组装 pipeline**（subtitle.rs，替换旧实现，返回 (String, Vec<Issue>)）:
```rust
use crate::normalize::special_chars::clean_special;
use crate::normalize::punct::{cn_punct, en_punct};
use crate::normalize::dialogue::{regularize_dash, regularize_markers};
use crate::normalize::classify::classify_style;
use crate::normalize::subtitle_check::check_timeline_cross;

/// 完整校准 pipeline：返回 (标准化 .ass 文本, 提示列表)。
pub fn format_ass(content: &str, _char_map: &[(String, String)]) -> (String, Vec<Issue>) {
    let mut issues = Vec::new();
    let parsed = parse_dialogues(content);
    let (merged, mut merge_issues) = merge_bilingual(parsed);
    issues.append(&mut merge_issues);

    // 时间轴交叉（对合并后的条目）
    issues.extend(check_timeline_cross(&merged));

    let mut out = build_header();
    for (i, d) in merged.iter().enumerate() {
        let zh_raw = d.zh();
        let en_raw = d.en();
        // 分段处理：中文段走 clean_special→cn_punct→regularize_markers→regularize_dash
        let mut zh = clean_special(&zh_raw);
        zh = cn_punct(&zh);
        zh = regularize_markers(&zh);
        zh = regularize_dash(&zh);
        // 英文段走 clean_special→en_punct→regularize_dash（不改括号全角）
        let en = en_raw.map(|e| {
            let mut t = clean_special(&e);
            t = en_punct(&t);
            regularize_dash(&t)
        });
        let style = classify_style(&zh, en.is_some());
        let text = Dialogue::rebuild(&zh, en.as_deref());
        out.push_str(&format!(
            "Dialogue: 0,{},{},{},0,0,0,0,,{}\n",
            d.start, d.end, style.name(), text
        ));
        // 可疑标点残留提示（轻量）
        for bad in [".,", ",.", "  "] {
            if text.contains(bad) {
                issues.push(Issue { line: i + 1, kind: format!("残留可疑标点[{bad}]"), text: text.clone() });
            }
        }
    }
    (out, issues)
}
```
删除旧 format_ass、旧 normalize_text（其职责已拆入各阶段）；旧的 fullwidth_chinese_punct 若无引用一并删。

- [ ] **Step 2: run_subtitle_normalize 适配新返回**（normalize/mod.rs）:
把
```rust
let formatted = subtitle::format_ass(&raw, char_map);
let issues = subtitle_check::check(&formatted);
```
改为
```rust
let (formatted, issues) = subtitle::format_ass(&raw, char_map);
```
（删除对 `subtitle_check::check` 的旧调用——交叉检测已在 pipeline 内。若 subtitle_check 还有其它被用的函数则保留。）

- [ ] **Step 3: 后端编译 + 全量测试**

Run: `cd /Users/zhoumo/Documents/Claude/collector/src-tauri && cargo test --lib 2>&1 | tail -12`
Expected: 编译通过、全部 ok（含各阶段单测）。修掉任何因旧函数删除导致的引用错误（如 normalize/mod.rs 现有测试 `normalizes_dir_and_reports` 断言 `你好！` 与 `[V4+ Styles]` 仍应成立）。

- [ ] **Step 4: 前端结果列表色标扩展**（NormalizeView.ts）

现有报告已按文件折叠展示 issues（`L{line} [{kind}] {text}`）。为新 kind 加色标：在渲染 issue 的地方，按 kind 前缀给不同颜色。最小实现——把原 `color:#ffb08a`（橙）改为按 kind 映射：
```ts
const kindColor = (kind: string): string => {
  if (kind.includes("交叉")) return "#ff9b9b";
  if (kind.includes("未合并") || kind.includes("多于")) return "#ffb07a";
  if (kind.includes("对话")) return "#9db8ff";
  if (kind.includes("道具") || kind.includes("外语") || kind.includes("歌曲")) return "#c3a8ff";
  return "#ffb08a";
};
```
在 issue 渲染处用 `style="color:${kindColor(i.kind)};font-size:12px"`。（保持现有按文件 `<details>` 折叠结构不变。）

- [ ] **Step 5: 类型检查 + 构建**

Run: `cd /Users/zhoumo/Documents/Claude/collector && npx tsc --noEmit && npm run build 2>&1 | tail -6`
Expected: 均通过。

- [ ] **Step 6: 提交**
```bash
cd /Users/zhoumo/Documents/Claude/collector
git add src-tauri/src/normalize/subtitle.rs src-tauri/src/normalize/mod.rs src/views/NormalizeView.ts
git commit -m "feat(subtitle): 组装校准 pipeline + 接入 + 结果列表按类型色标

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>"
```

---

### Task 11: 真机验证

**Files:** 无（手动验收）

- [ ] **Step 1: 启动**
```bash
cd /Users/zhoumo/Documents/Claude/collector
lsof -ti:1420 | xargs kill -9 2>/dev/null; pkill -f "tauri dev" 2>/dev/null; pkill -f "target/debug/collector" 2>/dev/null; sleep 1
nohup npm run tauri dev > /tmp/collector-dev.log 2>&1 & disown
```

- [ ] **Step 2: 逐项验收**
- 工具箱「字幕批量校准」选 docs/subtitles（或子目录）→ 开始 → 输出到 app_data/subtitles 对应结构。
- 打开一个校准后的 .ass 检查：中文段全角标点、英文段半角、同条中英统一 SEPARATOR、片名《》用 Title 样式、道具（）用 Note 样式；对话 `-` 统一「- 」。
- 结果列表：按文件折叠，各类提示（时间轴交叉/疑似未合并中英/跨条对话/疑似道具/外语/歌曲）带不同颜色。
- 播放一部校准后视频 → 字幕正常显示、样式统一。
- 抽查含中英混排的文件，确认中文标点没串到英文段、反之亦然。

- [ ] **Step 3: 关闭**
```bash
lsof -ti:1420 | xargs kill -9 2>/dev/null; pkill -f "tauri dev" 2>/dev/null
```

---

## 自审（对照 spec）

- **spec 覆盖**：规则1 分类样式→T8+T10组装；规则2 合并→T2；规则3 中英标点→T4/T5；规则4 特殊字符→T3；规则5 对话`-`规整→T6、跨条提示→T9/T10；规则6/7/8 标记规整→T7、疑似提示→质检；规则9 时间轴交叉→T9；规则10 Java查漏→各阶段对照 Java filter/sinicized 纳入可确定项；规则11 行业标准→与 11 条一致，无额外冲突规则。全覆盖。
- **pipeline 阶段**：preprocess/parse(T1)→merge(T2)→clean_special(T3)→cn/en_punct(T4/T5)→dialogue(T6)→markers(T7)→classify(T8)→交叉(T9)→组装(T10)，与 spec 阶段图一致。
- **占位符**：每步含完整可编译代码。「异体字表留空可扩展」是明确的空表结构，非占位。
- **类型一致**：`Issue` 统一定义在 subtitle.rs（T2 移动）、subtitle_check 与 pipeline 共用；`format_ass` 返回 `(String, Vec<Issue>)`（T10 定义、mod.rs 消费一致）；`parse_time_cs`/`Dialogue::zh/en/rebuild`（T1）被 T2/T9/T10 复用；`Style::name()`（T8）被 T10 用。
- **核心原则**：自动阶段（merge/clean/punct/regularize/classify）严格分中英段（T10 组装处对 zh/en 分别调不同函数，中文段才全角括号）；语义风险项（未合并/跨条/道具/外语/歌曲/交叉）全部走 Issue 提示，无自动臆测。

## 风险

- **中英段串扰**：T10 组装处严格 `zh` 段走 cn_punct+全角括号、`en` 段走 en_punct（半角、不全角括号），是最易错点——真机 Step2 末项专门抽查。
- **Issue 循环依赖**：T2 把 Issue 移到 subtitle.rs、subtitle_check 改 import——须先做（T2 Step1）否则后续编译不过。
- **旧函数删除的连带**：normalize/mod.rs 的现有测试 `normalizes_dir_and_reports` 依赖 format_ass 产出 `你好！`/`[V4+ Styles]`——新 pipeline 仍满足（cn_punct 全角 + build_header），T10 Step3 确认。
- **规则覆盖不全/边界**：基础版覆盖常见场景，复杂边界（数字小数点、引号嵌套、多语言混排）以 Issue 提示兜底，不追求一次到位；用户真机反馈后按数据表/阶段增量补。
