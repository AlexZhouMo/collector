# 字幕告警行内编辑器 设计

## 背景

工具箱「字幕批量校准」跑完后，`renderSubReport` 会按文件分组展示告警列表，每条告警含 `kind`（类型）、`line`（行号）、`text`（正文）。目前告警只能看，不能改——发现问题要退出应用、手动找到原始 `.ass` 文件、手动定位、手动改。

本功能让用户**双击一条告警，弹出编辑器，定位到目标行，人工编辑时间轴/正文并保存生效**。

## 关键现状（决定本设计的技术约束）

1. **告警的 `line` 是「校准后 merged 数组的第 N 条对白」，不是原始文件物理行号。** 校准过程 `merge_bilingual` 会把时间轴相同的中英两条合并成一条，并跳过特效空行。所以「告警第 6 条」在原文里可能对应第 12、13 两行 `Dialogue:`。
2. **告警的 `text` 是校准后的文本**（`format_ass` 已做全角化、破折号规整等），不是原文原样。
3. **校准输出写到 `app_data/subtitles/`**（保持相对目录结构），**不改动原始输入文件**（如 `docs/subtitles/…`）。`SubReport.file` 是相对路径（如 `剧集/a.ass`），可拼出输入路径与输出路径两者。
4. 校验函数 `check_timeline_cross` / `check_monolingual` / `check_song_symbol` 及残留标点检查都跑在校准后数据上，产出 `Issue { line, kind, text }`。

## 已确认的决策

- **编辑对象**：原始输入文件（`docs/subtitles/…`）。改动永久保留，下次校准会带上。
- **定位方式**：后端在校准时记录每条告警对应的原文物理行号并回传，编辑器直接拿到准确行号（不靠前端文本匹配）。
- **表格范围**：目标行 ±上下文（默认上下各 8 行），非整文件。
- **保存校验**：警告但允许保存（二次确认），不硬拦截——尊重人工判断的合理例外（如双语行时间轴故意相同）。
- **保存后列表**：对该文件重新校验，用新结果替换列表里该文件那一组告警。

## 架构总览

方案 A：后端给 `Dialogue`/`Issue` 带原文行号 + 两个新命令（读上下文、保存并重校）；前端一个模态编辑器组件 + store 支持单文件刷新。校验权威在后端（复用现成 `check_*`），前端只做即时预校验反馈。

（未采用方案 B「纯前端解析+校验」：会与后端两套校验逻辑迟早不一致；未采用方案 C「文本匹配定位」：校准后文本已被规整，匹配不稳。）

## 后端改动（Rust）

### 1. `Dialogue` 带原文行号

给 `Dialogue` 加字段 `src_lines: Vec<usize>`（这条对白来自原文的哪些物理行，1-based）。

- `parse_dialogues`：遍历 `preprocess(content).lines()` 时用 `enumerate` 记录**物理行号**（含被跳过的非 Dialogue 行也要计数，行号是原文真实行号），每条 Dialogue 的 `src_lines` 初始为 `vec![该行号]`。
- `merge_bilingual`：合并 group（时间轴相同的多条）时，把各条的 `src_lines` 合并（去重、升序）；未合并的直接沿用。
- 其余变换（`unify_same_row_sep` 等）不改变行数，`src_lines` 透传。

### 2. `Issue` 加字段

`Issue` 增加 `src_lines: Vec<usize>`。所有 `check_*` 和 `format_ass` 产 issue 时，从对应 Dialogue 取 `src_lines` 填入。`SubtitleReport` 序列化时随每条 issue 一起带回前端。

读取失败的兜底 issue（`line: 0, kind: "读取失败(编码无法识别)"`）`src_lines` 为空 `vec![]`。

### 3. 新命令 `read_subtitle_context`

```
read_subtitle_context(category, file, centerLines: Vec<usize>, radius: usize)
  -> Vec<ContextLine>
```

- 读**原始输入目录**下的 `<inputDir>/<file>`（`category` 用于必要时区分，`file` 为相对路径）。
- 以 `centerLines` 覆盖的物理行为中心，取上下各 `radius` 行范围内的所有 `Dialogue:` 行。
- 每行返回 `ContextLine { lineNo, start, end, text, isTarget }`，`isTarget = centerLines.contains(lineNo)`。
- 目标行读不到（文件被外部改动/删除、行号越界）→ 返回错误，前端提示「原文已变化，请重新校准」。

### 4. 新命令 `save_subtitle_edits`

```
save_subtitle_edits(category, file, edits: Vec<LineEdit>)
  -> Vec<SubIssue>          // 该文件重校后的新告警
```

`LineEdit { lineNo, start, end, text }`。

- 按 `lineNo` 定位原文对应 `Dialogue:` 行，**只替换其时间字段（Start/End）与 Text 字段**，保留 Layer/Style/Name/MarginL/R/V/Effect 等其余字段原样。非 Dialogue 行完全不动。
- 沿用 `encoding::read_subtitle` 的解码结果读原文，以 UTF-8 写回原始文件。
- 写回后对该文件重跑 `format_ass` 的校验部分，返回新的 issues（带 `src_lines`）。

## 前端改动（TS）

### 1. 告警条目可双击（事件委托）

`normalizeStore.renderSubReport`：每条 `.sub-issue` 加 `data-category` / `data-file` / `data-src-lines`（逗号分隔）/ `data-kind`，视觉上提示「双击编辑」。因结果区是 `innerHTML` 字符串，在字幕结果容器（`#sub-report`）挂一个 `dblclick` 委托监听：`e.target.closest(".sub-issue")` 读 data → 调 `openSubtitleEditor(...)`。读取失败类告警（`data-src-lines` 为空）双击提示「该文件无法解码，不能编辑」。

### 2. 新组件 `src/components/SubtitleEditor.ts`

模态弹窗（玻璃拟态，风格对齐现有 `MoveDialog` / `EditDrawer`）：

- 打开时调 `api.readSubtitleContext(category, file, srcLines, 8)`。
- **可编辑表格**：列 `行号 | 开始 | 结束 | 正文`。
  - 行号列只读、灰显。
  - 开始/结束/正文可编辑（`<input>`）。
  - 目标行高亮并自动滚入视野。
- 顶部：文件名 + 告警类型徽标（沿用 `kindColor`）。
- 底部：取消 / 保存。

### 3. 保存流程

1. 点保存 → 前端**即时预校验**（见下），有问题则相关单元格标红 + 顶部提示条列出。
2. 若有问题：弹二次确认「仍有 N 处问题，确定保存？」。
3. 确认后调 `api.saveSubtitleEdits(...)` 写回原文并重校，拿到该文件新 issues。
4. 调 `normalizeStore.updateFileIssues(file, issues)` 替换列表里该文件那组告警并重渲染；关闭弹窗；Toast 提示「已保存，该文件剩余 N 条提示」。

### 4. store 改动

`normalizeStore` 目前字幕结果只存 `resultHtml` 字符串。改为额外保存**结构化报告数据**（`SubReport[]`），新增 `updateFileIssues(file, issues)`：替换指定 file 的 issues 后用 `renderSubReport` 重渲染 `resultHtml` 并 `notify()`，使列表实时刷新。

## 校验与边界

### 前端预校验规则（即时反馈，与后端语义一致）

- **时间轴格式**：Start/End 必须匹配 `H:MM:SS.CS`（如 `0:00:42.66`），否则该单元格标红。
- **同行起止**：Start < End，否则标红。
- **时间轴交叉**：在**当前表格可见行范围内**按后端同规则检查（排序后每条与其后 3 条比区间重叠，起止完全相同的双语行不算交叉），交叉行标红。
- **正文异常格式**：复用后端判定——含 `,.`、连续两空格 `  `、异常 `.,`（点号前非字母，见 `has_abnormal_dot_comma`）→ 标红提示。

### 权威校验在后端

前端预校验只为体验；保存时后端 `save_subtitle_edits` 重跑校验，**以后端返回 issues 为准**刷新列表。前端只看可见窗口、后端看全文，两者可能略有差异，可接受。

### 边界处理

- **读取失败告警**（`src_lines` 空）：不可编辑，双击提示。
- **原文被外部改动/删除**：`read_subtitle_context` 读不到目标行 → 提示「原文已变化，请重新校准」。
- **src_lines 越界/为空**（理论不应发生）：降级为不自动定位，展示文件开头 ±上下文。
- **并发**：字幕任务运行中禁止打开编辑器，双击 Toast 提示「请等待校准完成」。

### 不做（YAGNI）

- 不做撤销/重做栈；不做多告警批量编辑；不做整文件全量编辑器（仅 ±上下文）；不做语法高亮。

## IPC 契约新增

```ts
interface ContextLine { lineNo: number; start: string; end: string; text: string; isTarget: boolean; }
interface LineEdit { lineNo: number; start: string; end: string; text: string; }
interface SubIssue { line: number; kind: string; text: string; src_lines: number[]; }  // 扩展

api.readSubtitleContext(category, file, centerLines, radius) -> ContextLine[]
api.saveSubtitleEdits(category, file, edits: LineEdit[])     -> SubIssue[]
```

## 测试要点

- 后端：`parse_dialogues` 行号正确（含跳过空行后的真实物理行号）；`merge_bilingual` 合并后 `src_lines` 含两条原文行；`save_subtitle_edits` 只改时间/正文两字段、保留其余字段与非 Dialogue 行；写回后重校 issues 正确。
- 前端：双击委托取对属性；预校验各规则标红；二次确认在有问题时弹出；保存后 `updateFileIssues` 正确替换单文件那组并刷新。
