# 字幕单语行检测（疑似漏加注释标记）Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 字幕批量校准时，对双语字幕文件检测「疑似漏加注释标记的裸单语行」（只有中文或只有英文、且整行未被 `（）`/`《》`/`[]`/`#` 包裹），在结果列表提示，供人工加标记修正。

**Architecture:** 纯后端改动，新增独立检测函数 `check_monolingual(&merged) -> Vec<Issue>`（仿现有 `check_timeline_cross` 范式），在 `format_ass` 里 `issues.extend(...)`。前端数据驱动零改动（新 `kind` 自动显示），仅可选为新 kind 加颜色。

**Tech Stack:** Rust (Tauri 2)。

**验证：** `cd src-tauri && cargo test && cargo build`；前端 `npx tsc --noEmit && npm run build`。

**判定规则（已与用户确认）：**
- **仅对双语文件生效**：文件里存在至少一条「有中文译文对 + 英文段」的对白（即某条 `en()` 非空且英文段含拉丁字母）→ 认定双语文件；纯中文文件整体跳过（不产单语提示）。
- 双语文件里，逐条：若为**单语行**（`en()` 为 None，即无译文分隔符），且**整行未被注释标记完整包裹**，则提示。
- **注释标记（整行 trim 后从头到尾被包裹才算正常，不提示）**：`（…）`、`《…》`、`[…]`、`［…］`、`#…#`。
- **两类都报**：
  - 单语行含中文（`has_cjk`）→ kind `"疑似漏译(仅中文)"`
  - 单语行不含中文但含拉丁字母（`has_latin`）→ kind `"疑似漏译(仅英文)"`
  - 既无中文也无拉丁字母（纯标点/数字/符号）→ 不报（避免误报）。

**范围外**：不碰工作树 440 项 `docs/subtitles/` 字幕数据变动。**提交时只 `git add` 明确的代码文件，不 `git add -A`。**

---

## Task 1: 新增单语行检测函数 check_monolingual

**Files:**
- Modify: `src-tauri/src/normalize/subtitle_check.rs`（新增 `check_monolingual` + 辅助判断 + 测试）

- [ ] **Step 1: 写失败测试**

在 `src-tauri/src/normalize/subtitle_check.rs` 末尾 `#[cfg(test)]` 区新增一个测试模块（放在文件最后）：

```rust
#[cfg(test)]
mod mono_tests {
    use super::*;
    use crate::normalize::subtitle::{Dialogue, SEPARATOR};

    fn bi(zh: &str, en: &str) -> Dialogue {
        Dialogue { start: "0:00:01.00".into(), end: "0:00:02.00".into(),
            text: format!("{zh}{SEPARATOR}{en}") }
    }
    fn mono(t: &str) -> Dialogue {
        Dialogue { start: "0:00:01.00".into(), end: "0:00:02.00".into(), text: t.into() }
    }

    #[test]
    fn bare_chinese_line_in_bilingual_file_flagged() {
        // 文件双语（有一条带英文译文），出现裸中文行「中国 北京」→ 报「疑似漏译(仅中文)」
        let ds = vec![ bi("你好", "Hi"), mono("中国 北京") ];
        let issues = check_monolingual(&ds);
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].kind, "疑似漏译(仅中文)");
        assert_eq!(issues[0].text, "中国 北京");
        assert_eq!(issues[0].line, 2);
    }

    #[test]
    fn bare_english_line_in_bilingual_file_flagged() {
        // 裸英文行（缺中文译文）→ 报「疑似漏译(仅英文)」
        let ds = vec![ bi("你好", "Hi"), mono("Beijing China") ];
        let issues = check_monolingual(&ds);
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].kind, "疑似漏译(仅英文)");
    }

    #[test]
    fn wrapped_notes_not_flagged() {
        // 被注释标记整行包裹的单语行都算正常：（中国 北京）《复仇者联盟》[咒语]#歌词#
        let ds = vec![
            bi("你好", "Hi"),
            mono("（中国 北京）"),
            mono("《复仇者联盟》"),
            mono("[咒语]"),
            mono("［咒语］"),
            mono("#歌词#"),
        ];
        assert!(check_monolingual(&ds).is_empty());
    }

    #[test]
    fn partial_wrap_still_flagged() {
        // 只有部分带括号（整行没被包裹）仍视为裸文本 → 报
        let ds = vec![ bi("你好", "Hi"), mono("中国（北京）人") ];
        let issues = check_monolingual(&ds);
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].kind, "疑似漏译(仅中文)");
    }

    #[test]
    fn pure_chinese_file_skipped() {
        // 纯中文文件（无任何英文译文对）→ 整体跳过，裸中文行不报
        let ds = vec![ mono("你好"), mono("中国 北京"), mono("再见") ];
        assert!(check_monolingual(&ds).is_empty());
    }

    #[test]
    fn bilingual_rows_not_flagged() {
        // 正常双语行（有译文）不报；破折号对话行同理
        let ds = vec![
            bi("你好", "Hi"),
            bi("- 叫车 - 我不叫", "- Get a cab. - I never get cabs."),
        ];
        assert!(check_monolingual(&ds).is_empty());
    }

    #[test]
    fn punct_only_line_not_flagged() {
        // 既无中文也无拉丁字母（纯标点/数字）→ 不报，避免误报
        let ds = vec![ bi("你好", "Hi"), mono("...123..."), mono("♪♪♪") ];
        assert!(check_monolingual(&ds).is_empty());
    }
}
```

- [ ] **Step 2: 运行测试确认失败**

Run: `cd src-tauri && cargo test check_monolingual 2>&1 | tail -20`
Expected: 编译失败（`check_monolingual` 未定义）。

- [ ] **Step 3: 实现 check_monolingual + 辅助函数**

在 `src-tauri/src/normalize/subtitle_check.rs`，`check_timeline_cross` 函数之后、`#[cfg(test)]` 之前，新增：

```rust
/// 是否含拉丁字母（英文判据）。
fn has_latin(s: &str) -> bool {
    s.chars().any(|c| c.is_ascii_alphabetic())
}

/// 是否含中文（与 subtitle.rs 的 has_cjk 同范围，本文件内私有副本，避免暴露内部函数）。
fn has_cjk(s: &str) -> bool {
    s.chars().any(|c| ('\u{4e00}'..='\u{9fa5}').contains(&c))
}

/// 整行（trim 后）是否被一对注释标记从头到尾包裹：（…）《…》[…]［…］#…#。
/// 用于识别「标题/旁白/外语/歌词」等正常的单语行，不算漏译。
fn is_fully_wrapped(s: &str) -> bool {
    let t = s.trim();
    let chars: Vec<char> = t.chars().collect();
    if chars.len() < 2 { return false; }
    let (first, last) = (chars[0], chars[chars.len() - 1]);
    matches!((first, last),
        ('（', '）') | ('(', ')') | ('《', '》') |
        ('[', ']') | ('［', '］') | ('#', '#'))
}

/// 单语行检测（疑似漏加注释标记）：仅当文件为双语（存在至少一条带英文译文的对白）时生效。
/// 逐条：无译文（en() 为 None）且整行未被注释标记包裹的裸单语行 → 提示。
/// 含中文报「疑似漏译(仅中文)」；不含中文但含拉丁字母报「疑似漏译(仅英文)」；
/// 二者皆无（纯标点/数字）不报。
pub fn check_monolingual(dialogues: &[Dialogue]) -> Vec<Issue> {
    // 双语文件判定：任一条有英文译文段（含拉丁字母）
    let is_bilingual_file = dialogues.iter().any(|d|
        d.en().map(|e| has_latin(&e)).unwrap_or(false));
    if !is_bilingual_file { return Vec::new(); }

    let mut issues = Vec::new();
    for (i, d) in dialogues.iter().enumerate() {
        if d.en().is_some() { continue; }          // 有译文 → 双语行，跳过
        let zh = d.zh();
        if is_fully_wrapped(&zh) { continue; }      // 被注释标记包裹 → 正常
        let kind = if has_cjk(&zh) {
            "疑似漏译(仅中文)"
        } else if has_latin(&zh) {
            "疑似漏译(仅英文)"
        } else {
            continue;                                // 纯标点/数字 → 不报
        };
        issues.push(Issue { line: i + 1, kind: kind.into(), text: d.text.clone() });
    }
    issues
}
```

- [ ] **Step 4: 运行测试确认通过**

Run: `cd src-tauri && cargo test check_monolingual 2>&1 | tail -20; cargo test mono_tests 2>&1 | grep "test result:"`
Expected: mono_tests 全过。

- [ ] **Step 5: 提交**

```bash
cd /Users/zhoumo/Documents/Claude/collector
git add src-tauri/src/normalize/subtitle_check.rs
git commit -m "feat(subtitle): check_monolingual 检测双语文件里疑似漏加注释标记的裸单语行

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>"
```

---

## Task 2: 接入 format_ass pipeline

**Files:**
- Modify: `src-tauri/src/normalize/subtitle.rs`（import + extend）

- [ ] **Step 1: 写集成测试**

在 `src-tauri/src/normalize/subtitle.rs` 的 `mod build_tests` 里新增（放在 `english_side_stays_halfwidth` 之后）：

```rust
    #[test]
    fn monolingual_bare_line_flagged_in_pipeline() {
        // 双语文件里出现裸中文行「中国 北京」→ format_ass 应产出「疑似漏译(仅中文)」提示
        let ass = "[Events]\n\
            Dialogue: 0,0:00:01.00,0:00:02.00,Default,,0,0,0,,你好\\N{\\fnArial\\fs30}Hi\n\
            Dialogue: 0,0:00:03.00,0:00:04.00,Default,,0,0,0,,中国 北京\n";
        let (_, issues) = format_ass(ass, &[]);
        assert!(issues.iter().any(|i| i.kind == "疑似漏译(仅中文)"),
            "应检出裸中文行: {issues:?}");
    }

    #[test]
    fn monolingual_wrapped_note_not_flagged_in_pipeline() {
        // 双语文件里被括号包裹的旁白（中国 北京）→ 正常，不产漏译提示
        let ass = "[Events]\n\
            Dialogue: 0,0:00:01.00,0:00:02.00,Default,,0,0,0,,你好\\N{\\fnArial\\fs30}Hi\n\
            Dialogue: 0,0:00:03.00,0:00:04.00,Default,,0,0,0,,（中国 北京）\n";
        let (_, issues) = format_ass(ass, &[]);
        assert!(!issues.iter().any(|i| i.kind.contains("漏译")),
            "被括号包裹不应报漏译: {issues:?}");
    }
```

- [ ] **Step 2: 运行确认失败**

Run: `cd src-tauri && cargo test monolingual_bare_line_flagged_in_pipeline 2>&1 | tail -15`
Expected: FAIL（断言不满足，pipeline 尚未接入 check_monolingual）。

- [ ] **Step 3: 接入 check_monolingual**

`src-tauri/src/normalize/subtitle.rs` 第 5 行 import 追加 `check_monolingual`：

```rust
use crate::normalize::subtitle_check::{check_timeline_cross, check_monolingual};
```

在 `format_ass` 里 `issues.extend(check_timeline_cross(&merged));`（约 203 行）之后加一行：

```rust
    issues.extend(check_monolingual(&merged));
```

- [ ] **Step 4: 运行确认通过**

Run: `cd src-tauri && cargo test 2>&1 | grep "test result:" | head -5`
Expected: 全部测试通过（含新增两条 pipeline 测试 + Task 1 的 mono_tests）。

- [ ] **Step 5: 提交**

```bash
git add src-tauri/src/normalize/subtitle.rs
git commit -m "feat(subtitle): format_ass 接入 check_monolingual，双语文件裸单语行进结果列表

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>"
```

---

## Task 3: 前端 kind 颜色（可选微调）

**Files:**
- Modify: `src/lib/normalizeStore.ts`（kindColor 加分支）

- [ ] **Step 1: 给「漏译」kind 加专属色**

`src/lib/normalizeStore.ts` 的 `kindColor`（约 63-69 行），在 return 默认色之前加一分支：

```ts
  if (kind.includes("漏译")) return "#ffd479";
```

（放在现有 `if` 链末尾、`return "#ffb08a"` 之前。其余分支不动。）

- [ ] **Step 2: 类型检查 + 构建**

Run: `cd /Users/zhoumo/Documents/Claude/collector && npx tsc --noEmit && echo tsc-ok; npm run build 2>&1 | tail -1`
Expected: tsc-ok + 构建成功。

- [ ] **Step 3: 提交**

```bash
git add src/lib/normalizeStore.ts
git commit -m "feat(subtitle): 漏译提示 kind 加专属高亮色

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>"
```

---

## Task 4: 端到端验证

- [ ] **Step 1: 全套编译测试**

Run: `cd src-tauri && cargo test 2>&1 | grep "test result:" | head; cargo build 2>&1 | grep -iE "error|warning" || echo clean; cd .. && npx tsc --noEmit && echo tsc-ok; npm run build 2>&1 | tail -1`
Expected: 测试全过 + clean + tsc-ok + 构建成功。

- [ ] **Step 2: 真机跑一次字幕校准（用户执行 / dev 已运行）**

`npm run tauri:dev` 起应用，工具箱 →「字幕批量校准」→ 开始校准。确认：
- 双语文件里的裸单语行（无括号/书名号的纯中文或纯英文行）出现在结果列表，kind 为「疑似漏译(仅中文)」/「疑似漏译(仅英文)」。
- 被 `（）`/`《》`/`[]`/`#` 包裹的行不报。
- 纯中文文件（国产片/日剧）不产漏译提示。

- [ ] **Step 3: 工作树确认（只含本次代码变更 + 既有字幕变动）**

Run: `git status --short | grep -v "docs/subtitles"; git log --oneline -4`
Expected: 除 docs/subtitles 外工作树干净；本次 2-3 个提交在列。
