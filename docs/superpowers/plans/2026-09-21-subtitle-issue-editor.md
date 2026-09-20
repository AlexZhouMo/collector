# 字幕告警行内编辑器 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 让「字幕批量校准」告警列表支持双击弹出编辑器，定位到原始字幕文件的目标行，人工编辑时间轴/正文并保存生效，保存后重校该文件刷新列表。

**Architecture:** 后端在解析时给 `Dialogue`/`Issue` 带上原文物理行号并随报告回传；新增两个 Tauri 命令：读目标行±上下文、保存编辑并重校单文件。前端新增玻璃拟态模态编辑器组件（可编辑表格），保存时前端即时预校验+二次确认，写回后由后端重校，用新结果替换 store 里该文件的告警组。校验权威在后端（复用现成 `check_*`）。

**Tech Stack:** Rust (Tauri 2, serde, encoding_rs, walkdir)、TypeScript (Vite, 原生 DOM)。

---

## 关键现状（实现者必读）

- `Issue { line, kind, text }` 定义在 `src-tauri/src/normalize/subtitle.rs:10-15`，`line` 是**校准后 merged 数组序号**（非原文物理行号），`text` 是**校准后文本**。
- `Dialogue { start, end, text }` 定义在 `subtitle.rs:18-23`。
- `parse_dialogues`（`subtitle.rs:159-181`）遍历 `preprocess(content).lines()`，只收 `Dialogue:` 开头且非空文本的行。
- `merge_bilingual`（`subtitle.rs:78-122`）把时间轴完全相同的多条合并成一条。
- `format_ass`（`subtitle.rs:198-240`）是校准 pipeline，产出 `(标准化文本, Vec<Issue>)`。
- 校验函数在 `src-tauri/src/normalize/subtitle_check.rs`：`check_timeline_cross`、`check_monolingual`、`check_song_symbol`。
- `SubtitleReport { file, issues }` 定义在 `src-tauri/src/normalize/mod.rs:18-22`，`file` 是相对输入目录的路径（如 `剧集/a.ass`）。
- `normalize_subtitles`（`mod.rs:76-99`）的 `in_dir` 是前端传入的字符串；输出写到 `app_data/subtitles/`。**编辑对象是原始输入文件**，定位靠 `inputDir + file`（`inputDir` 前端已持有为 `subIn`，见 `NormalizeView.ts:85,158`）。
- `encoding::read_subtitle(path: &Path) -> Option<String>`（`encoding.rs:33-36`）。
- 命令注册在 `src-tauri/src/lib.rs:673-708`（`generate_handler!` 列表）。
- 前端 IPC 在 `src/lib/ipc.ts`；`SubIssue`/`SubReport` 在 `ipc.ts:17-18`。
- 字幕结果渲染 `renderSubReport` 在 `src/lib/normalizeStore.ts:121-134`；`kindColor` 在 `normalizeStore.ts:63-70`。
- 模态弹窗模式：`.modal-overlay` + `.glass`，见 `MoveDialog.ts:36-46`。Toast：`showToast(msg, "info"|"error")`（`Toast.ts`）。HTML 转义：`esc()`（`escape.ts`）。

---

## 文件结构

**后端（Rust）**
- 修改 `src-tauri/src/normalize/subtitle.rs`：`Dialogue` 加 `src_lines`；`Issue` 加 `src_lines`；`parse_dialogues` 记录物理行号；`merge_bilingual` 合并 `src_lines`；`format_ass` 及各 issue 填 `src_lines`。
- 修改 `src-tauri/src/normalize/subtitle_check.rs`：三个 `check_*` 填 `src_lines`。
- 修改 `src-tauri/src/normalize/mod.rs`：`SubtitleReport` 兜底 issue 的 `src_lines`；新增 `read_subtitle_context`、`save_subtitle_edits` 两个命令与其纯函数实现。
- 修改 `src-tauri/src/lib.rs`：注册两个新命令。

**前端（TS）**
- 修改 `src/lib/ipc.ts`：`SubIssue` 加 `src_lines`；新增 `ContextLine`、`LineEdit` 类型与两个 api 方法。
- 新增 `src/lib/subtitleValidate.ts`：前端预校验纯函数（时间轴格式、起止、交叉、正文异常）。
- 新增 `src/components/SubtitleEditor.ts`：模态编辑器组件 `openSubtitleEditor(...)`。
- 修改 `src/lib/normalizeStore.ts`：`renderSubReport` 给 issue 加 data 属性；保存结构化 `SubReport[]`；新增 `updateFileIssues(file, issues)`。
- 修改 `src/views/NormalizeView.ts`：给 `#sub-report` 挂 `dblclick` 委托监听，调 `openSubtitleEditor`。

---

## Task 1: `Dialogue` 带原文行号，`parse_dialogues` 记录物理行号

**Files:**
- Modify: `src-tauri/src/normalize/subtitle.rs`
- Test: `src-tauri/src/normalize/subtitle.rs`（内联 `#[cfg(test)]`）

- [ ] **Step 1: 写失败测试**

在 `subtitle.rs` 的 `mod blank_tests`（约 `subtitle.rs:358`）后追加：

```rust
#[cfg(test)]
mod src_line_tests {
    use super::*;
    #[test]
    fn parse_records_physical_line_numbers() {
        // 第1行 [Events]、第2行 Dialogue、第3行空文本特效行(跳过)、第4行 Dialogue
        let ass = "[Events]\n\
Dialogue: 0,0:00:01.00,0:00:02.00,Default,,0,0,0,,你好\n\
Dialogue: 0,0:00:01.50,0:00:02.50,Default,,0,0,0,,{\\pos(1,2)}\n\
Dialogue: 0,0:00:03.00,0:00:04.00,Default,,0,0,0,,世界\n";
        let ds = parse_dialogues(ass);
        assert_eq!(ds.len(), 2);
        assert_eq!(ds[0].src_lines, vec![2]); // 「你好」在物理第2行
        assert_eq!(ds[1].src_lines, vec![4]); // 「世界」在物理第4行(第3行被跳过但仍占行号)
    }
}
```

- [ ] **Step 2: 运行测试确认失败**

Run: `cd src-tauri && cargo test src_line_tests 2>&1 | tail -20`
Expected: 编译失败，`Dialogue` 无字段 `src_lines`。

- [ ] **Step 3: 给 `Dialogue` 加字段并在 `parse_dialogues` 填充**

修改 `subtitle.rs:18-23` 的 `Dialogue`：

```rust
/// 一条对白：时间戳区间 + 文本（可能含中英，用 SEPARATOR 分隔）。
#[derive(Debug, Clone)]
pub struct Dialogue {
    pub start: String,
    pub end: String,
    pub text: String,
    /// 这条对白来自原文的物理行号（1-based）。合并后可能含多个。
    pub src_lines: Vec<usize>,
}
```

修改 `parse_dialogues`（`subtitle.rs:159-181`）用 `enumerate` 记录物理行号：

```rust
pub fn parse_dialogues(content: &str) -> Vec<Dialogue> {
    let mut out = Vec::new();
    for (idx, line) in preprocess(content).lines().enumerate() {
        if !line.starts_with("Dialogue:") {
            continue;
        }
        let rest = &line["Dialogue:".len()..];
        let parts: Vec<&str> = rest.splitn(10, ',').collect();
        if parts.len() < 10 {
            continue;
        }
        let text = parts[9].to_string();
        if is_blank_dialogue(&text) {
            continue;
        }
        out.push(Dialogue {
            start: parts[1].trim().to_string(),
            end: parts[2].trim().to_string(),
            text,
            src_lines: vec![idx + 1], // 物理行号 1-based
        });
    }
    out
}
```

- [ ] **Step 4: 修所有构造 `Dialogue` 的测试辅助函数**

`subtitle.rs` 内多处测试用字面量构造 `Dialogue`，需补 `src_lines`。逐个改：

- `mono_tests`/`cross_tests`/`song_tests` 在 `subtitle_check.rs` 里（Task 3 处理），本文件只改 `subtitle.rs` 内的：
- `time_tests`（`subtitle.rs:330-336`）两处 `Dialogue { ... }`：各加 `src_lines: vec![1],`。
- `merge_tests` 的 `fn d`（`subtitle.rs:387`）：

```rust
fn d(s:&str,e:&str,t:&str)->Dialogue{Dialogue{start:s.into(),end:e.into(),text:t.into(),src_lines:vec![1]}}
```

- [ ] **Step 5: 运行测试确认通过**

Run: `cd src-tauri && cargo test src_line_tests 2>&1 | tail -20`
Expected: `test result: ok`，`parse_records_physical_line_numbers` PASS。

- [ ] **Step 6: 提交**

```bash
git add src-tauri/src/normalize/subtitle.rs
git commit -m "feat(subtitle): Dialogue 记录原文物理行号 src_lines

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>"
```

---

## Task 2: `merge_bilingual` 合并 `src_lines`

**Files:**
- Modify: `src-tauri/src/normalize/subtitle.rs`
- Test: `src-tauri/src/normalize/subtitle.rs`

- [ ] **Step 1: 写失败测试**

在 `mod merge_tests`（`subtitle.rs:385`）内追加：

```rust
#[test]
fn merge_combines_src_lines() {
    // 两条时间轴完全相同的中英行 → 合并为一条，src_lines 含两条原文行
    let mut zh = d("0:00:01.00","0:00:02.00","你好");
    zh.src_lines = vec![2];
    let mut en = d("0:00:01.00","0:00:02.00","Hello");
    en.src_lines = vec![3];
    let (out, _) = merge_bilingual(vec![zh, en]);
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].src_lines, vec![2, 3]);
}
```

- [ ] **Step 2: 运行确认失败**

Run: `cd src-tauri && cargo test merge_combines_src_lines 2>&1 | tail -20`
Expected: FAIL，`out[0].src_lines` 为 `vec![2]`（未合并 en 的行号）。

- [ ] **Step 3: 在合并分支合并 `src_lines`**

修改 `merge_bilingual`（`subtitle.rs:92-105`）的合并分支。原 `else` 分支构造合并 Dialogue 处补 `src_lines`：

```rust
        if group.len() == 1 {
            out.push(ds[i].clone());
        } else {
            let zh_first = group.iter().find(|g| has_cjk(&g.text)).copied().unwrap_or(group[0]);
            let en_part = group.iter().find(|g| !has_cjk(&g.text)).map(|g| g.text.clone());
            let zh_seg = zh_first.text.split(SEPARATOR).next().unwrap_or(&zh_first.text).trim();
            let mut merged_lines: Vec<usize> =
                group.iter().flat_map(|g| g.src_lines.iter().copied()).collect();
            merged_lines.sort_unstable();
            merged_lines.dedup();
            out.push(Dialogue {
                start: ds[i].start.clone(), end: ds[i].end.clone(),
                text: Dialogue::rebuild(zh_seg, en_part.as_deref()),
                src_lines: merged_lines,
            });
            if group.len() > 2 {
                issues.push(Issue { line: out.len(), kind: "同时间轴多于2条".into(), text: ds[i].text.clone(), src_lines: out.last().unwrap().src_lines.clone() });
            }
        }
```

> 注：`Issue` 此时还没有 `src_lines` 字段 —— Task 4 才加。为避免本任务编译失败，本步**先只改合并 Dialogue 的 `src_lines`**，`Issue { ... }` 那行**暂不加 `src_lines`**（保持原样 `Issue { line: out.len(), kind: "同时间轴多于2条".into(), text: ds[i].text.clone() }`）。`Issue` 的 `src_lines` 统一在 Task 4 加。

即本步实际改动只到：

```rust
            let mut merged_lines: Vec<usize> =
                group.iter().flat_map(|g| g.src_lines.iter().copied()).collect();
            merged_lines.sort_unstable();
            merged_lines.dedup();
            out.push(Dialogue {
                start: ds[i].start.clone(), end: ds[i].end.clone(),
                text: Dialogue::rebuild(zh_seg, en_part.as_deref()),
                src_lines: merged_lines,
            });
            if group.len() > 2 {
                issues.push(Issue { line: out.len(), kind: "同时间轴多于2条".into(), text: ds[i].text.clone() });
            }
```

- [ ] **Step 4: 运行确认通过**

Run: `cd src-tauri && cargo test merge_ 2>&1 | tail -20`
Expected: `merge_combines_src_lines` PASS，其余 merge 测试仍 PASS。

- [ ] **Step 5: 提交**

```bash
git add src-tauri/src/normalize/subtitle.rs
git commit -m "feat(subtitle): merge_bilingual 合并 src_lines

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>"
```

---

## Task 3: 三个 `check_*` 填 `src_lines`（先备好 Dialogue，Issue 字段 Task 4 加）

**Files:**
- Modify: `src-tauri/src/normalize/subtitle_check.rs`
- Test: `src-tauri/src/normalize/subtitle_check.rs`

- [ ] **Step 1: 先修测试辅助函数构造 Dialogue**

`subtitle_check.rs` 里三个测试模块的辅助函数构造 `Dialogue` 需补 `src_lines`：

- `cross_tests` 的 `fn d`（`subtitle_check.rs:117`）：

```rust
fn d(s:&str,e:&str)->Dialogue{Dialogue{start:s.into(),end:e.into(),text:"x".into(),src_lines:vec![1]}}
```

- `mono_tests` 的 `fn bi` / `fn mono`（`subtitle_check.rs:148-154`）：

```rust
fn bi(zh: &str, en: &str) -> Dialogue {
    Dialogue { start: "0:00:01.00".into(), end: "0:00:02.00".into(),
        text: format!("{zh}{SEPARATOR}{en}"), src_lines: vec![1] }
}
fn mono(t: &str) -> Dialogue {
    Dialogue { start: "0:00:01.00".into(), end: "0:00:02.00".into(), text: t.into(), src_lines: vec![1] }
}
```

- `song_tests` 的 `fn line`（`subtitle_check.rs:246`）：

```rust
fn line(t: &str) -> Dialogue {
    Dialogue { start: "0:00:01.00".into(), end: "0:00:02.00".into(), text: t.into(), src_lines: vec![1] }
}
```

- [ ] **Step 2: 运行确认现有测试仍能编译通过**

Run: `cd src-tauri && cargo test -p collector normalize::subtitle_check 2>&1 | tail -20`
Expected: 全部 PASS（此步只补字段不改逻辑）。

- [ ] **Step 3: 提交（Issue.src_lines 逻辑留待 Task 4）**

```bash
git add src-tauri/src/normalize/subtitle_check.rs
git commit -m "chore(subtitle): check 测试辅助补 src_lines 字段

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>"
```

---

## Task 4: `Issue` 加 `src_lines`，所有产 issue 处填充

**Files:**
- Modify: `src-tauri/src/normalize/subtitle.rs`、`src-tauri/src/normalize/subtitle_check.rs`
- Test: `src-tauri/src/normalize/subtitle_check.rs`

- [ ] **Step 1: 写失败测试**

在 `subtitle_check.rs` 的 `mod cross_tests`（`subtitle_check.rs:113`）内追加：

```rust
#[test]
fn cross_issue_carries_src_lines() {
    let mut a = d("0:00:01.00","0:00:05.00"); a.src_lines = vec![10];
    let mut b = d("0:00:04.00","0:00:06.00"); b.src_lines = vec![11];
    let issues = check_timeline_cross(&[a, b]);
    let cross = issues.iter().find(|i| i.kind == "时间轴交叉").unwrap();
    assert_eq!(cross.src_lines, vec![11]); // 报后一条 b 的原文行
}
```

- [ ] **Step 2: 运行确认失败**

Run: `cd src-tauri && cargo test cross_issue_carries_src_lines 2>&1 | tail -20`
Expected: 编译失败，`Issue` 无 `src_lines`。

- [ ] **Step 3: 给 `Issue` 加字段**

修改 `subtitle.rs:10-15` 的 `Issue`：

```rust
/// 质检/合并过程中发现的可疑行。
#[derive(Debug, Clone, serde::Serialize)]
pub struct Issue {
    pub line: usize,
    pub kind: String,
    pub text: String,
    /// 对应原文物理行号（1-based，可能多行）。用于编辑器定位。
    pub src_lines: Vec<usize>,
}
```

- [ ] **Step 4: 填充 subtitle.rs 内所有 Issue 构造**

- `merge_bilingual` 的「同时间轴多于2条」（Task 2 Step 3 保留原样处，`subtitle.rs` 约 103 行）：

```rust
            if group.len() > 2 {
                issues.push(Issue { line: out.len(), kind: "同时间轴多于2条".into(), text: ds[i].text.clone(), src_lines: out.last().unwrap().src_lines.clone() });
            }
```

- `merge_bilingual` 的「疑似未合并中英」（`subtitle.rs:116`）：

```rust
                    issues.push(Issue { line: k + 1, kind: "疑似未合并中英".into(), text: b.text.clone(), src_lines: b.src_lines.clone() });
```

- `format_ass` 里残留标点两处（`subtitle.rs:232,236`）——此处 `d` 是当前遍历的 merged Dialogue：

```rust
        for bad in [",.", "  "] {
            if text.contains(bad) {
                issues.push(Issue { line: i + 1, kind: format!("残留可疑标点[{bad}]"), text: text.clone(), src_lines: d.src_lines.clone() });
            }
        }
        if has_abnormal_dot_comma(&text) {
            issues.push(Issue { line: i + 1, kind: "残留可疑标点[.,]".into(), text: text.clone(), src_lines: d.src_lines.clone() });
        }
```

- [ ] **Step 5: 填充 subtitle_check.rs 内所有 Issue 构造**

- `check_timeline_cross`（`subtitle_check.rs:22-26`）：

```rust
                issues.push(Issue {
                    line: idx[b] + 1,
                    kind: "时间轴交叉".into(),
                    text: dialogues[idx[b]].text.clone(),
                    src_lines: dialogues[idx[b]].src_lines.clone(),
                });
```

- `check_monolingual`（`subtitle_check.rs:88`）：

```rust
        issues.push(Issue { line: i + 1, kind: kind.into(), text: d.text.clone(), src_lines: d.src_lines.clone() });
```

- `check_song_symbol`（`subtitle_check.rs:103-107`）：

```rust
            issues.push(Issue {
                line: i + 1,
                kind: "非标准歌曲符(建议改∮)".into(),
                text: d.text.clone(),
                src_lines: d.src_lines.clone(),
            });
```

- [ ] **Step 6: 运行确认通过**

Run: `cd src-tauri && cargo test normalize:: 2>&1 | tail -25`
Expected: 全部 PASS，含 `cross_issue_carries_src_lines`。

- [ ] **Step 7: 提交**

```bash
git add src-tauri/src/normalize/subtitle.rs src-tauri/src/normalize/subtitle_check.rs
git commit -m "feat(subtitle): Issue 带 src_lines，所有校验填原文行号

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>"
```

---

## Task 5: `SubtitleReport` 兜底 issue 补 `src_lines`

**Files:**
- Modify: `src-tauri/src/normalize/mod.rs`

- [ ] **Step 1: 补字段**

`mod.rs:50-56` 的读取失败兜底 issue 加 `src_lines: vec![]`：

```rust
                reports.push(SubtitleReport {
                    file: rel.to_string_lossy().into_owned(),
                    issues: vec![subtitle_check::Issue {
                        line: 0,
                        kind: "读取失败(编码无法识别)".into(),
                        text: String::new(),
                        src_lines: vec![],
                    }],
                });
```

- [ ] **Step 2: 运行确认现有 mod 测试通过**

Run: `cd src-tauri && cargo test normalize::tests 2>&1 | tail -15`
Expected: `normalizes_dir_and_reports`、`skips_system_junk_files` PASS。

- [ ] **Step 3: 提交**

```bash
git add src-tauri/src/normalize/mod.rs
git commit -m "fix(subtitle): 读取失败兜底 issue 补 src_lines

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>"
```

---

## Task 6: 后端命令 `read_subtitle_context`

**Files:**
- Modify: `src-tauri/src/normalize/mod.rs`
- Test: `src-tauri/src/normalize/mod.rs`

- [ ] **Step 1: 写失败测试**

在 `mod.rs` 的 `mod tests`（`mod.rs:112`）内追加。测试直接测纯函数 `collect_context`（命令是薄包装）：

```rust
#[test]
fn context_returns_target_and_neighbors() {
    let ass = "[Events]\n\
Dialogue: 0,0:00:01.00,0:00:02.00,Default,,0,0,0,,A\n\
Dialogue: 0,0:00:03.00,0:00:04.00,Default,,0,0,0,,B\n\
Dialogue: 0,0:00:05.00,0:00:06.00,Default,,0,0,0,,C\n";
    // 目标物理行 3（B），radius 1 → 覆盖物理行 2..=4 内的 Dialogue = A,B,C
    let lines = collect_context(ass, &[3], 1);
    assert_eq!(lines.len(), 3);
    assert_eq!(lines[1].text, "B");
    assert!(lines[1].is_target);
    assert!(!lines[0].is_target);
    assert_eq!(lines[0].line_no, 2);
    assert_eq!(lines[1].line_no, 3);
}
```

- [ ] **Step 2: 运行确认失败**

Run: `cd src-tauri && cargo test context_returns 2>&1 | tail -15`
Expected: 编译失败，`collect_context`/`ContextLine` 未定义。

- [ ] **Step 3: 实现 `ContextLine` + `collect_context` + 命令**

在 `mod.rs` 顶部（`use` 之后，`SubtitleReport` 附近）加类型与纯函数：

```rust
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ContextLine {
    pub line_no: usize,
    pub start: String,
    pub end: String,
    pub text: String,
    pub is_target: bool,
}

/// 从 .ass 文本收集：以 center_lines 覆盖范围为中心、上下各 radius 物理行内的所有 Dialogue 行。
/// line_no 为原文物理行号(1-based)。text 为原文 Text 字段(未规整)。
pub fn collect_context(content: &str, center_lines: &[usize], radius: usize) -> Vec<ContextLine> {
    use crate::normalize::subtitle::preprocess;
    let lo = center_lines.iter().copied().min().unwrap_or(1).saturating_sub(radius).max(1);
    let hi = center_lines.iter().copied().max().unwrap_or(1).saturating_add(radius);
    let mut out = Vec::new();
    for (idx, line) in preprocess(content).lines().enumerate() {
        let no = idx + 1;
        if no < lo || no > hi { continue; }
        if !line.starts_with("Dialogue:") { continue; }
        let rest = &line["Dialogue:".len()..];
        let parts: Vec<&str> = rest.splitn(10, ',').collect();
        if parts.len() < 10 { continue; }
        out.push(ContextLine {
            line_no: no,
            start: parts[1].trim().to_string(),
            end: parts[2].trim().to_string(),
            text: parts[9].to_string(),
            is_target: center_lines.contains(&no),
        });
    }
    out
}
```

> 注：`preprocess` 目前是 `subtitle.rs` 内 `pub fn`（`subtitle.rs:125`），可直接 `use`。

命令（放在 `subtitle_output_dir` 之后，`mod.rs:110` 附近）：

```rust
/// 读原始输入文件 in_dir/file，返回目标行±radius 范围内的 Dialogue 行。
#[tauri::command(rename_all = "camelCase")]
pub fn read_subtitle_context(
    in_dir: String,
    file: String,
    center_lines: Vec<usize>,
    radius: usize,
) -> AppResult<Vec<ContextLine>> {
    let path = Path::new(&in_dir).join(&file);
    let raw = encoding::read_subtitle(&path)
        .ok_or_else(|| crate::error::AppError::Other("原文无法读取或解码".into()))?;
    let lines = collect_context(&raw, &center_lines, radius);
    if lines.is_empty() {
        return Err(crate::error::AppError::Other("原文已变化，请重新校准".into()));
    }
    Ok(lines)
}
```

- [ ] **Step 4: 运行确认通过**

Run: `cd src-tauri && cargo test context_returns 2>&1 | tail -15`
Expected: `context_returns_target_and_neighbors` PASS。

- [ ] **Step 5: 注册命令**

`lib.rs:707` 的 handler 列表在 `maintenance::clean_covers` 后补：

```rust
            maintenance::clean_covers,
            normalize::read_subtitle_context,
```

- [ ] **Step 6: 提交**

```bash
git add src-tauri/src/normalize/mod.rs src-tauri/src/lib.rs
git commit -m "feat(subtitle): read_subtitle_context 命令(读目标行上下文)

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>"
```

---

## Task 7: 后端命令 `save_subtitle_edits`

**Files:**
- Modify: `src-tauri/src/normalize/mod.rs`
- Test: `src-tauri/src/normalize/mod.rs`

- [ ] **Step 1: 写失败测试**

在 `mod tests` 内追加。测纯函数 `apply_edits`（按物理行替换 Start/End/Text，保留其余字段与非 Dialogue 行）：

```rust
#[test]
fn apply_edits_replaces_only_time_and_text() {
    let ass = "[Events]\n\
Dialogue: 0,0:00:01.00,0:00:02.00,Title,NAME,5,6,7,fx,你好\n\
Dialogue: 0,0:00:03.00,0:00:04.00,Default,,0,0,0,,世界\n";
    let edits = vec![LineEdit {
        line_no: 2,
        start: "0:00:01.50".into(),
        end: "0:00:02.50".into(),
        text: "您好".into(),
    }];
    let out = apply_edits(ass, &edits);
    // 第2行时间与正文改，Style=Title/NAME/margins/fx 保留
    assert!(out.contains("Dialogue: 0,0:00:01.50,0:00:02.50,Title,NAME,5,6,7,fx,您好"));
    // 第3行完全不动
    assert!(out.contains("Dialogue: 0,0:00:03.00,0:00:04.00,Default,,0,0,0,,世界"));
    // 头部保留
    assert!(out.starts_with("[Events]"));
}
```

- [ ] **Step 2: 运行确认失败**

Run: `cd src-tauri && cargo test apply_edits_replaces 2>&1 | tail -15`
Expected: 编译失败，`LineEdit`/`apply_edits` 未定义。

- [ ] **Step 3: 实现 `LineEdit` + `apply_edits` + 命令**

在 `mod.rs` 类型区加：

```rust
#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LineEdit {
    pub line_no: usize,
    pub start: String,
    pub end: String,
    pub text: String,
}

/// 按物理行号替换 Dialogue 行的 Start/End/Text 字段，保留其余字段与所有非 Dialogue 行。
/// 输出以 \n 分隔（preprocess 已统一换行）。
pub fn apply_edits(content: &str, edits: &[LineEdit]) -> String {
    use crate::normalize::subtitle::preprocess;
    use std::collections::HashMap;
    let map: HashMap<usize, &LineEdit> = edits.iter().map(|e| (e.line_no, e)).collect();
    let processed = preprocess(content);
    let mut out_lines: Vec<String> = Vec::new();
    for (idx, line) in processed.lines().enumerate() {
        let no = idx + 1;
        match map.get(&no) {
            Some(e) if line.starts_with("Dialogue:") => {
                let rest = &line["Dialogue:".len()..];
                let parts: Vec<&str> = rest.splitn(10, ',').collect();
                if parts.len() == 10 {
                    // Layer,Start,End,Style,Name,MarginL,MarginR,MarginV,Effect,Text
                    out_lines.push(format!(
                        "Dialogue:{},{},{},{},{},{},{},{},{},{}",
                        parts[0], e.start, e.end, parts[3], parts[4],
                        parts[5], parts[6], parts[7], parts[8], e.text
                    ));
                } else {
                    out_lines.push(line.to_string());
                }
            }
            _ => out_lines.push(line.to_string()),
        }
    }
    let mut s = out_lines.join("\n");
    s.push('\n');
    s
}
```

> 注：原 `Dialogue:` 后第一个字段前无空格分隔时（如 `Dialogue: 0,...` 有空格），`splitn` 的 `parts[0]` 会是 ` 0`（带前导空格）。为保持与原文一致，上面用 `format!("Dialogue:{},...", parts[0], ...)` 直接拼回 `parts[0]`（含其原有前导空格），不额外加空格。

命令（放在 `read_subtitle_context` 之后）：

```rust
/// 把 edits 按物理行写回原始文件 in_dir/file，再对该文件重跑校验，返回新 issues。
#[tauri::command(rename_all = "camelCase")]
pub fn save_subtitle_edits(
    in_dir: String,
    file: String,
    edits: Vec<LineEdit>,
) -> AppResult<Vec<subtitle_check::Issue>> {
    let path = Path::new(&in_dir).join(&file);
    let raw = encoding::read_subtitle(&path)
        .ok_or_else(|| crate::error::AppError::Other("原文无法读取或解码".into()))?;
    let edited = apply_edits(&raw, &edits);
    std::fs::write(&path, edited.as_bytes())?;
    // 重校：对写回后的内容重跑 format_ass，取其 issues
    let (_, issues) = subtitle::format_ass(&edited, &[]);
    Ok(issues)
}
```

- [ ] **Step 4: 运行确认通过**

Run: `cd src-tauri && cargo test apply_edits_replaces 2>&1 | tail -15`
Expected: `apply_edits_replaces_only_time_and_text` PASS。

- [ ] **Step 5: 注册命令**

`lib.rs` handler 列表在 `read_subtitle_context` 后补：

```rust
            normalize::read_subtitle_context,
            normalize::save_subtitle_edits,
```

- [ ] **Step 6: 全量后端测试 + 编译**

Run: `cd src-tauri && cargo test 2>&1 | tail -20 && cargo build 2>&1 | tail -5`
Expected: 全部测试 PASS，`cargo build` 成功。

- [ ] **Step 7: 提交**

```bash
git add src-tauri/src/normalize/mod.rs src-tauri/src/lib.rs
git commit -m "feat(subtitle): save_subtitle_edits 命令(写回原文并重校)

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>"
```

---

## Task 8: 前端 IPC 类型与方法

**Files:**
- Modify: `src/lib/ipc.ts`

- [ ] **Step 1: 扩展 `SubIssue` 并加新类型/方法**

`ipc.ts:17` 的 `SubIssue` 加 `src_lines`：

```ts
interface SubIssue { line: number; kind: string; text: string; src_lines: number[]; }
export interface SubReport { file: string; issues: SubIssue[]; }
export type { SubIssue };
```

在 `api` 对象前加类型：

```ts
export interface ContextLine { lineNo: number; start: string; end: string; text: string; isTarget: boolean; }
export interface LineEdit { lineNo: number; start: string; end: string; text: string; }
```

在 `api` 对象内（`normalizeSubtitles` 附近，`ipc.ts:45` 后）加：

```ts
  readSubtitleContext: (inDir: string, file: string, centerLines: number[], radius: number) =>
    invoke<ContextLine[]>("read_subtitle_context", { inDir, file, centerLines, radius }),
  saveSubtitleEdits: (inDir: string, file: string, edits: LineEdit[]) =>
    invoke<SubIssue[]>("save_subtitle_edits", { inDir, file, edits }),
```

- [ ] **Step 2: 类型检查**

Run: `npx tsc --noEmit 2>&1 | tail -15`
Expected: 无新错误（`SubIssue` 已 export，供 store/editor 用）。

- [ ] **Step 3: 提交**

```bash
git add src/lib/ipc.ts
git commit -m "feat(subtitle): 前端 IPC 加 context/edits 类型与方法

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>"
```

---

## Task 9: 前端预校验纯函数 `subtitleValidate.ts`

**Files:**
- Create: `src/lib/subtitleValidate.ts`

- [ ] **Step 1: 实现**

```ts
// 编辑器保存前的即时预校验（与后端 subtitle_check 语义一致）。
// 权威校验仍在后端；此处仅为即时反馈。

export interface RowInput { lineNo: number; start: string; end: string; text: string; }
export interface RowProblem { lineNo: number; field: "start" | "end" | "text"; msg: string; }

const TIME_RE = /^\d:\d{2}:\d{2}\.\d{2}$/;

/** "H:MM:SS.CS" → 厘秒；非法返回 null。 */
export function parseTimeCs(t: string): number | null {
  if (!TIME_RE.test(t)) return null;
  const [hms, cs] = t.split(".");
  const [h, m, s] = hms.split(":").map(Number);
  return ((h * 60 + m) * 60 + s) * 100 + Number(cs);
}

/** 正文异常：含 ",." 或连续两空格；或异常 ".,"（点号前非字母）。 */
export function textProblem(text: string): string | null {
  if (text.includes(",.")) return '含可疑标点 ",."';
  if (text.includes("  ")) return "含连续两个空格";
  for (let i = 0; i < text.length; i++) {
    if (text[i] === "." && text[i + 1] === ",") {
      const prev = text[i - 1];
      if (!prev || !/[A-Za-z]/.test(prev)) return '含可疑标点 ".,"';
    }
  }
  return null;
}

/** 时间轴交叉：排序后每条与其后 3 条比区间重叠；起止完全相同不算交叉。 */
function crossProblems(rows: RowInput[]): RowProblem[] {
  const withCs = rows
    .map((r) => ({ r, s: parseTimeCs(r.start), e: parseTimeCs(r.end) }))
    .filter((x) => x.s !== null && x.e !== null) as { r: RowInput; s: number; e: number }[];
  withCs.sort((a, b) => a.s - b.s);
  const out: RowProblem[] = [];
  for (let a = 0; a < withCs.length; a++) {
    for (let b = a + 1; b < Math.min(a + 4, withCs.length); b++) {
      const A = withCs[a], B = withCs[b];
      const identical = A.s === B.s && A.e === B.e;
      if (!identical && A.s < B.e && B.s < A.e) {
        out.push({ lineNo: B.r.lineNo, field: "start", msg: "时间轴与相邻行交叉" });
      }
    }
  }
  return out;
}

/** 全部预校验：返回问题列表（空 = 无问题）。 */
export function validateRows(rows: RowInput[]): RowProblem[] {
  const problems: RowProblem[] = [];
  for (const r of rows) {
    if (parseTimeCs(r.start) === null) problems.push({ lineNo: r.lineNo, field: "start", msg: "时间格式非法" });
    if (parseTimeCs(r.end) === null) problems.push({ lineNo: r.lineNo, field: "end", msg: "时间格式非法" });
    const cs = parseTimeCs(r.start), ce = parseTimeCs(r.end);
    if (cs !== null && ce !== null && cs >= ce) problems.push({ lineNo: r.lineNo, field: "end", msg: "结束不晚于开始" });
    const tp = textProblem(r.text);
    if (tp) problems.push({ lineNo: r.lineNo, field: "text", msg: tp });
  }
  problems.push(...crossProblems(rows));
  return problems;
}
```

- [ ] **Step 2: 类型检查**

Run: `npx tsc --noEmit 2>&1 | tail -15`
Expected: 无错误。

- [ ] **Step 3: 提交**

```bash
git add src/lib/subtitleValidate.ts
git commit -m "feat(subtitle): 编辑器预校验纯函数

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>"
```

---

## Task 10: store 存结构化报告并支持单文件刷新

**Files:**
- Modify: `src/lib/normalizeStore.ts`

- [ ] **Step 1: `renderSubReport` 给 issue 加 data 属性**

修改 `normalizeStore.ts:127-132` 的 issue 渲染，让每条 `.sub-issue` 带定位数据。注意 `renderSubReport` 需要能访问 `file`（当前 forEach 的 `r.file`）：

```ts
  reports.filter(r => r.issues.length).forEach(r => {
    const issuesHtml = r.issues.map(i => {
      const editable = i.src_lines && i.src_lines.length > 0;
      const srcAttr = editable ? i.src_lines.join(",") : "";
      const hint = editable ? ' title="双击编辑"' : ' title="该文件无法解码，不能编辑"';
      return `<div class="sub-issue${editable ? " editable" : ""}" data-file="${esc(r.file)}" data-src-lines="${srcAttr}" data-kind="${esc(i.kind)}"${hint}><span class="sub-kind" style="--k:${kindColor(i.kind)}">${esc(i.kind)}</span><span class="sub-loc">L${i.line}</span><span class="sub-text">${esc(i.text)}</span></div>`;
    }).join("");
    html += `<details class="sub-file"><summary><span class="sub-fname">${esc(r.file)}</span><span class="sub-badge">${r.issues.length}</span></summary><div class="sub-issues">${issuesHtml}</div></details>`;
  });
```

- [ ] **Step 2: 保存结构化报告**

在文件顶部 import 补 `SubReport` 已有（`ipc.ts` import）。给 subtitle 任务保存原始 reports。在模块级加：

```ts
// 最近一次字幕报告的结构化数据（供单文件重校刷新）。
let lastSubReports: SubReport[] = [];
```

修改 `startSubtitle`（`normalizeStore.ts:180-194`）成功分支：

```ts
    const reports = await api.normalizeSubtitles(dir);
    lastSubReports = reports;
    s.pct = 100;
    s.statusText = `完成：处理 ${reports.length} 个文件`;
    s.resultHtml = renderSubReport(reports);
    s.status = "done";
```

- [ ] **Step 3: 新增 `updateFileIssues`**

在文件末尾（`startComic` 后）加导出：

```ts
/** 保存编辑后：用重校得到的新 issues 替换指定 file 那一组，重渲染字幕结果并通知。 */
export function updateFileIssues(file: string, issues: SubReport["issues"]): void {
  const idx = lastSubReports.findIndex((r) => r.file === file);
  if (idx >= 0) lastSubReports[idx] = { file, issues };
  const s = states.subtitle;
  s.resultHtml = renderSubReport(lastSubReports);
  notify();
}
```

- [ ] **Step 4: 类型检查**

Run: `npx tsc --noEmit 2>&1 | tail -15`
Expected: 无错误。

- [ ] **Step 5: 提交**

```bash
git add src/lib/normalizeStore.ts
git commit -m "feat(subtitle): store 存结构化报告，支持单文件重校刷新

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>"
```

---

## Task 11: 编辑器组件 `SubtitleEditor.ts`

**Files:**
- Create: `src/components/SubtitleEditor.ts`

- [ ] **Step 1: 实现组件**

```ts
import { api } from "../lib/ipc";
import type { LineEdit } from "../lib/ipc";
import { esc } from "../lib/escape";
import { showToast } from "./Toast";
import { validateRows, type RowInput, type RowProblem } from "../lib/subtitleValidate";
import { kindColor } from "../lib/normalizeStore";
import { updateFileIssues } from "../lib/normalizeStore";

interface OpenOpts {
  inDir: string;
  file: string;
  srcLines: number[];
  kind: string;
}

/** 双击告警打开：读原文目标行±8 行，可编辑表格，保存前预校验+二次确认，写回后重校刷新列表。 */
export async function openSubtitleEditor(opts: OpenOpts): Promise<void> {
  const { inDir, file, srcLines, kind } = opts;
  let lines;
  try {
    lines = await api.readSubtitleContext(inDir, file, srcLines, 8);
  } catch (e) {
    showToast(String(e), "error");
    return;
  }

  const overlay = document.createElement("div");
  overlay.className = "modal-overlay";
  const dialog = document.createElement("div");
  dialog.className = "sub-editor glass";
  overlay.appendChild(dialog);

  const close = () => {
    overlay.remove();
    document.removeEventListener("keydown", onKey, true);
  };
  const onKey = (e: KeyboardEvent) => { if (e.key === "Escape") close(); };
  document.addEventListener("keydown", onKey, true);

  const rowsHtml = lines.map((l) => `
    <tr data-line="${l.lineNo}"${l.isTarget ? ' class="target"' : ""}>
      <td class="se-no">${l.lineNo}</td>
      <td><input class="se-start" value="${esc(l.start)}" /></td>
      <td><input class="se-end" value="${esc(l.end)}" /></td>
      <td><input class="se-text" value="${esc(l.text)}" /></td>
    </tr>`).join("");

  dialog.innerHTML = `
    <div class="se-head">
      <span class="se-file" title="${esc(file)}">${esc(file)}</span>
      <span class="se-kind" style="--k:${kindColor(kind)}">${esc(kind)}</span>
    </div>
    <div class="se-problems" style="display:none"></div>
    <div class="se-table-wrap">
      <table class="se-table">
        <thead><tr><th>行号</th><th>开始</th><th>结束</th><th>正文</th></tr></thead>
        <tbody>${rowsHtml}</tbody>
      </table>
    </div>
    <div class="se-actions">
      <button class="se-cancel">取消</button>
      <button class="btn-primary se-save">保存</button>
    </div>`;

  document.body.appendChild(overlay);
  overlay.addEventListener("mousedown", (e) => { if (e.target === overlay) close(); });
  dialog.querySelector<HTMLButtonElement>(".se-cancel")!.onclick = close;

  // 目标行滚入视野
  const targetTr = dialog.querySelector<HTMLElement>("tr.target");
  targetTr?.scrollIntoView({ block: "center" });

  const collectRows = (): RowInput[] =>
    Array.from(dialog.querySelectorAll<HTMLElement>("tbody tr")).map((tr) => ({
      lineNo: Number(tr.dataset.line),
      start: tr.querySelector<HTMLInputElement>(".se-start")!.value.trim(),
      end: tr.querySelector<HTMLInputElement>(".se-end")!.value.trim(),
      text: tr.querySelector<HTMLInputElement>(".se-text")!.value,
    }));

  const paintProblems = (problems: RowProblem[]) => {
    dialog.querySelectorAll(".se-start,.se-end,.se-text").forEach((el) => el.classList.remove("bad"));
    const box = dialog.querySelector<HTMLElement>(".se-problems")!;
    if (!problems.length) { box.style.display = "none"; box.innerHTML = ""; return; }
    for (const p of problems) {
      const tr = dialog.querySelector<HTMLElement>(`tr[data-line="${p.lineNo}"]`);
      tr?.querySelector(`.se-${p.field}`)?.classList.add("bad");
    }
    box.style.display = "block";
    box.innerHTML = problems.map((p) => `<div>L${p.lineNo}：${esc(p.msg)}</div>`).join("");
  };

  const doSave = async () => {
    const rows = collectRows();
    const edits: LineEdit[] = rows.map((r) => ({ lineNo: r.lineNo, start: r.start, end: r.end, text: r.text }));
    try {
      const issues = await api.saveSubtitleEdits(inDir, file, edits);
      updateFileIssues(file, issues);
      close();
      showToast(`已保存，该文件剩余 ${issues.length} 条提示`);
    } catch (e) {
      showToast(String(e), "error");
    }
  };

  dialog.querySelector<HTMLButtonElement>(".se-save")!.onclick = () => {
    const problems = validateRows(collectRows());
    paintProblems(problems);
    if (problems.length && !confirm(`仍有 ${problems.length} 处问题，确定保存？`)) return;
    void doSave();
  };
}
```

- [ ] **Step 2: 导出 `kindColor`**

`normalizeStore.ts:63` 的 `kindColor` 当前是模块私有 `const`。改为导出：

```ts
export const kindColor = (kind: string): string => {
```

- [ ] **Step 3: 类型检查**

Run: `npx tsc --noEmit 2>&1 | tail -15`
Expected: 无错误。

- [ ] **Step 4: 提交**

```bash
git add src/components/SubtitleEditor.ts src/lib/normalizeStore.ts
git commit -m "feat(subtitle): 行内编辑器组件 SubtitleEditor

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>"
```

---

## Task 12: NormalizeView 挂双击委托

**Files:**
- Modify: `src/views/NormalizeView.ts`

- [ ] **Step 1: 挂 `dblclick` 委托**

`NormalizeView.ts` 顶部加 import：

```ts
import { openSubtitleEditor } from "../components/SubtitleEditor";
import { getState } from "../lib/normalizeStore";
```

> 注：`getState` 已在现有 import（`normalizeStore.ts` 导出，`NormalizeView.ts:5` 已 import 了 `getState`）。若已 import 则不重复加，只加 `openSubtitleEditor`。

在 `syncFromStore()` 定义后、`return el` 前（`NormalizeView.ts:196` 附近）加：

```ts
  // 字幕告警双击 → 打开行内编辑器（事件委托，结果区是 innerHTML 字符串）
  $("#sub-report").addEventListener("dblclick", (e) => {
    const row = (e.target as HTMLElement).closest<HTMLElement>(".sub-issue");
    if (!row) return;
    if (getState("subtitle").status === "running") { showToast("请等待校准完成"); return; }
    const srcRaw = row.dataset.srcLines ?? "";
    if (!srcRaw) { showToast("该文件无法解码，不能编辑", "error"); return; }
    const srcLines = srcRaw.split(",").map(Number).filter((n) => n > 0);
    if (!srcLines.length) { showToast("无法定位原文行", "error"); return; }
    openSubtitleEditor({
      inDir: subIn || "docs/subtitles",
      file: row.dataset.file ?? "",
      srcLines,
      kind: row.dataset.kind ?? "",
    });
  });
```

`NormalizeView.ts` 顶部若无 `showToast` import，补：

```ts
import { showToast } from "../components/Toast";
```

- [ ] **Step 2: 类型检查 + 前端构建**

Run: `npx tsc --noEmit 2>&1 | tail -15 && npm run build 2>&1 | tail -8`
Expected: 无类型错误，`vite build` 成功。

- [ ] **Step 3: 提交**

```bash
git add src/views/NormalizeView.ts
git commit -m "feat(subtitle): 告警双击打开行内编辑器

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>"
```

---

## Task 13: 编辑器样式

**Files:**
- Modify: `src/styles/theme.css`（或现有放 `.sub-issue`/`.modal-overlay` 样式的文件）

- [ ] **Step 1: 定位现有样式文件**

Run: `grep -rl "modal-overlay\|sub-issue\|move-dialog" src/styles/ 2>/dev/null`
Expected: 输出承载模态与字幕结果样式的 CSS 文件路径（记为 `<CSS>`）。

- [ ] **Step 2: 追加编辑器样式**

在 `<CSS>` 末尾追加（玻璃拟态、与 `.move-dialog` 一致的观感）：

```css
/* 字幕行内编辑器 */
.sub-editor {
  width: min(760px, 92vw);
  max-height: 82vh;
  display: flex;
  flex-direction: column;
  padding: 18px;
  border-radius: 14px;
  gap: 12px;
}
.se-head { display: flex; align-items: center; gap: 10px; }
.se-file { font-size: 13px; color: var(--text-dim); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; flex: 1; }
.se-kind { font-size: 12px; padding: 2px 8px; border-radius: 6px; color: var(--k); border: 1px solid var(--k); }
.se-problems { font-size: 12px; color: #ff9b9b; background: rgba(255,80,80,.08); border-radius: 8px; padding: 8px 10px; line-height: 1.7; }
.se-table-wrap { overflow: auto; flex: 1; border: 1px solid var(--border); border-radius: 10px; }
.se-table { width: 100%; border-collapse: collapse; font-size: 13px; }
.se-table th { position: sticky; top: 0; background: var(--glass); color: var(--text-dim); font-weight: 500; padding: 8px 10px; text-align: left; }
.se-table td { padding: 4px 8px; border-top: 1px solid var(--border); }
.se-table tr.target { background: rgba(120,150,255,.12); }
.se-no { color: var(--text-dim); text-align: right; width: 48px; font-variant-numeric: tabular-nums; }
.se-table input { width: 100%; box-sizing: border-box; background: transparent; border: 1px solid transparent; border-radius: 6px; color: var(--text); padding: 4px 6px; font-size: 13px; }
.se-table input:focus { border-color: var(--accent); background: var(--glass); outline: none; }
.se-table input.se-start, .se-table input.se-end { width: 120px; font-variant-numeric: tabular-nums; }
.se-table input.bad { border-color: #ff6b6b; background: rgba(255,80,80,.1); }
.se-actions { display: flex; justify-content: flex-end; gap: 10px; }
.se-actions button { padding: 7px 16px; border-radius: 8px; border: 1px solid var(--border); background: var(--glass); color: var(--text); cursor: pointer; }
.se-actions .btn-primary { border-color: var(--accent); }
.sub-issue.editable { cursor: pointer; }
.sub-issue.editable:hover { background: var(--glass); border-radius: 6px; }
```

- [ ] **Step 3: 前端构建**

Run: `npm run build 2>&1 | tail -6`
Expected: `vite build` 成功。

- [ ] **Step 4: 提交**

```bash
git add src/styles/
git commit -m "style(subtitle): 行内编辑器样式

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>"
```

---

## Task 14: 端到端手动验证（Tauri dev）

**Files:** 无（验证任务）

- [ ] **Step 1: 启动 Tauri dev**

Run: `npm run tauri:dev`（后台，等原生窗口）
Expected: 编译成功，窗口弹出。

- [ ] **Step 2: 走一遍流程**

- 工具箱 → 字幕批量校准 → 选一个含双语/交叉/歌曲符问题的目录 → 开始校准。
- 告警列表出现，`.editable` 条目 hover 有反馈。
- 双击一条「时间轴交叉」告警 → 编辑器弹出，目标行高亮居中，上下文可见。
- 改一处正文引入连续两空格 → 保存 → 顶部提示条标红 + 二次确认弹出。
- 确认保存 → Toast 显示「已保存，该文件剩余 N 条提示」，列表该文件那组刷新。
- 双击读取失败类告警 → Toast「该文件无法解码，不能编辑」。
- 校准运行中双击 → Toast「请等待校准完成」。

- [ ] **Step 3: 确认原文被正确写回**

Run: 打开被编辑的原始 `.ass`，确认目标 Dialogue 行的时间/正文已改、Style 等其余字段与非 Dialogue 行未变。
Expected: 只改了预期字段。

- [ ] **Step 4: 全量测试兜底**

Run: `cd src-tauri && cargo test 2>&1 | tail -8`
Expected: 全部 PASS。

---

## Self-Review 记录

- **Spec 覆盖**：编辑原文（Task 7 写回 in_dir/file）✓；后端回传行号（Task 1/2/4 src_lines）✓；±上下文（Task 6 radius=8）✓；警告允许保存（Task 11 confirm）✓；重校单文件（Task 7 重跑 format_ass + Task 10 updateFileIssues）✓；预校验规则（Task 9）✓；写回只改时间+正文两字段（Task 7 apply_edits）✓；边界（读取失败/原文变化/并发，Task 11/12）✓；YAGNI（无撤销栈/批量/全量编辑器）✓。
- **命令签名修正**：设计文档写 `category`，实际 `SubReport` 无 category、`file` 为相对路径、定位需 `inDir`。计划统一用 `inDir + file`。
- **类型一致**：`SubIssue.src_lines`（Task 8）与后端 `Issue.src_lines`（Task 4，serde 默认字段名 `src_lines`）一致；`ContextLine` 用 `#[serde(rename_all="camelCase")]` → 前端 `lineNo/isTarget`（Task 6/8）一致；`LineEdit` 同为 camelCase（Task 7/8）；`kindColor` 导出名一致（Task 10/11）。
