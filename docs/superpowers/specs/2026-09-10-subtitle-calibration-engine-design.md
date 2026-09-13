# 字幕批量校准规则引擎 设计

日期：2026-09-10
状态：设计已确认，待写实现计划

## 背景与问题

工具箱「字幕批量校准」目前复用旧的 `normalize::subtitle::format_ass`，只做了皮毛：BOM/换行预处理、Dialogue 解析、中文标点部分全角化、统一 ASS 头（Default/Title/Note 三样式）、少量质检（时间轴逆序、几个可疑标点）。用户要求的 11 条完整规则里，**合并中英分离行、特殊字符/OCR 表、对话 `-` 规整、跨条 `...`、非对话括号、外语方括号、歌曲 #、引号「」配对、Title/Note 分类、英文标点体系、时间线交叉检测**等核心逻辑尚未实现。

参考 `demo/SubtitlesFormat.java`（merge/filter/sinicized/check/build）与 `SubtitlesSearch.java`（质检规则库）理解意图——但 Java 源码中文与特殊字符已被 GBK 编码损坏成乱码，仅作意图参考，不照抄。

## 核心原则（贯穿全部规则）

**能确定的自动改，有语义判断风险的只提示不臆测。**
- 自动：统一样式、合并（时间轴完全相同）、中英标点各自体系、特殊字符/OCR 表、规整已标注的对话 `-`/括号/方括号/#、分类（靠《》()确定字符标志）。
- 仅提示（进执行结果列表，人工核对）：疑似未合并中英对（时间轴微小差异）、跨条对话 `...`、可疑对话结构、疑似未标注的道具翻译/外语/歌曲、时间轴交叉、特殊字符残留。

## 决策汇总

| 项 | 决策 |
|----|------|
| 架构 | pipeline：有序纯函数阶段，每阶段独立单测；特殊字符数据驱动（映射表） |
| 合并(规则2) | 时间轴**完全相同**的多条 Dialogue 合并为「中文 SEPARATOR 英文」；同条 `\N` 拆开重组统一 SEPARATOR |
| 微小差异 | 相邻两条 start 和 end 差都 ≤ 0.5s 且一中一英 → **提示**「疑似未合并中英对」，不自动合并 |
| 特殊字符(规则4) | 分表数据驱动：特殊字符归一表 / OCR英文纠错表 / 标点规整正则 + 上下文规则函数；基础版可扩展，不还原 Java 损坏映射 |
| 对话`-`(规则5) | **规整已标注的 `-`**（自动，中英对齐、统一空格）；跨条 `...`、可疑对话结构**仅提示** |
| 非对话括号(6)/外语方括号(7)/歌曲#(8) | **规整已标注的**（自动统一格式）；疑似未标注的**仅提示** |
| 分类+样式(规则1) | 一份文件内逐条自适应：有英文→中英双行样式，纯中→单行；三分类 Title(含《》)/Note(被()括起)/Default(其余)，用现有三样式 |
| 中英标点(规则3) | 中文段→中文标点体系（全角），英文段→英文标点体系（半角+规整），各自独立处理 |
| 引号 | 中文段成对 `"` → 「」配对（奇偶切换），行尾特殊处理（参考 Java sinicized） |
| 时间线交叉(规则9) | 先按时间排序，每条与**前后各 3 条**检查区间重叠 → **提示**「时间轴交叉」 |
| Java 查漏(规则10) | 逐条对照 Java filter/sinicized/check，把可确定的规则纳入对应阶段（见「阶段」） |
| 行业标准(规则11) | 采用通用双语字幕共识：中文在上英文在下、中文全角标点、英文半角、对话破折号、外语/歌词标记——与用户 11 条一致，无冲突，不额外引入争议规则 |
| 结果列表 | 按文件分组、组内类型色标+位置+摘要、可折叠（延续现有报告风格） |
| 输入/输出 | 沿用上轮：输入目录可配（默认 docs/subtitles），输出固定 `<app_data>/subtitles/` 按结构 |

## 架构：pipeline 阶段

`format_ass(content) -> (标准化文本, Vec<Issue>)`，内部按序执行。每阶段纯函数、独立单测。

```
1. preprocess            去 BOM、统一换行（已有，复用）
2. parse_dialogues       解析为 [Dialogue{start,end,text,style?}]（已有，扩展）
3. merge_bilingual       时间轴完全相同的合并为 中文+SEP+英文；同条 \N 拆开重组
                         → 副产：检测微小差异未合并对 → Issue
4. clean_special         特殊字符归一表 + OCR英文纠错表 + 标点规整正则（数据驱动）
                         → 副产：残留特殊字符 → Issue
5. normalize_cn_punct    中文段：半角标点→全角体系（，。！？：…）；成对 `"` → 「」奇偶配对、行尾未闭合容错
6. normalize_en_punct    英文段：标点规整（半角、连续点→…、多空格、行尾补句号等）
7. regularize_dialogue   规整已标注的对话 `-`（中英各自「- 」空格对齐）
                         → 副产：跨条对话/可疑对话结构 → Issue
8. regularize_markers    规整已标注的 ()（非对话）、[]（外语）、#（歌曲）格式
                         → 副产：疑似未标注 → Issue
9. classify_style        分类 Title(《》)/Note(())/Default，选样式；有英文→双行，纯中→单行
10. build_output         统一 ASS 头 + 逐条按样式组装
—— 独立质检 ——
check_timeline_cross     排序后每条与前后 3 条查区间重叠 → Issue
```

`Dialogue` 结构扩展：增加 `style`（Default/Title/Note）与便于各阶段读写的 `zh`/`en` 拆分（或保留 text + SEPARATOR 约定，阶段内 split）。具体在实现计划定，保持每阶段接口清晰。

`Issue { file, line, kind, text }`：kind 枚举含「时间轴交叉」「疑似未合并中英」「跨条对话」「可疑对话结构」「疑似道具翻译」「疑似外语」「疑似歌曲」「残留特殊字符」等。

## 组件与文件

- `src-tauri/src/normalize/subtitle.rs`：重构为 pipeline。现有 preprocess/parse_dialogues/build_header 保留复用；normalize_text/fullwidth_chinese_punct 拆解并入对应阶段。
- `src-tauri/src/normalize/subtitle/`（若文件过大，拆子模块）：`merge.rs`、`special_chars.rs`（数据表）、`punct.rs`（中/英标点）、`dialogue.rs`、`markers.rs`、`classify.rs`。是否拆分在实现计划按体量定；原则：每文件一个阶段职责、可独立测。
- `src-tauri/src/normalize/subtitle_check.rs`：时间轴交叉改为「排序+前后3条窗口」；逆序检测并入。质检 Issue 与 pipeline 副产 Issue 合并。
- `src-tauri/src/normalize/mod.rs`：`run_subtitle_normalize` 汇总每文件的 (formatted, issues)，报告结构 `SubtitleReport{file, issues}` 已有，issues 现在更丰富。
- 前端 `NormalizeView.ts`：结果列表已按文件折叠展示 issues，扩展 kind 的色标即可（沿用现有 `.pt-*` 或新增少量 class）。

## 特殊字符数据表（规则 4，基础版）

以数据表 `&[(from, to)]` + 正则规则表达，基础版包含（从 Java 可确定推断的）：
- **OCR 英文纠错**（带词边界）：` l ` → ` I `、`,,l ` / `}l ` / `"l ` 行首 → `I `、`lt'`→`It'`、`lsn'`→`Isn'`、` i ` → ` I ` 等。
- **标点规整**：`--` → `…`、`''` → `"`、连续 `.{3,}` → `…`、`.,`/`,.` 清理、多空格→单空格、`? `/`! ` 后空格规整。
- **可扩展**：异体字/全角拉丁映射留空表结构，用户日后遇到实际错字往表里加（Java 损坏的乱码映射不还原）。

## 数据流

1. 工具箱选输入目录 → 开始校准 → 后端 `run_subtitle_normalize` 遍历 .ass。
2. 每文件：读文本 → pipeline（自动改 + 收集 Issue）→ 写标准化文本到 `<app_data>/subtitles/<相对结构>`。
3. 汇总所有文件的 Issue → 返回 `Vec<SubtitleReport>`。
4. 前端按文件分组折叠展示（类型色标 + 位置 + 摘要）。

## 错误处理

- 单文件解析/处理异常：记为该文件一条 Issue（kind「处理失败」），跳过不中断整批。
- 时间戳格式非法（无法解析成秒）：交叉检测跳过该条，记 Issue 提示。
- 空文件/无 Dialogue：输出仅含头，不报错。

## 测试策略

- **每阶段纯函数单测**（cargo test）：
  - merge：完全相同时间轴合并、同条 `\N` 重组、微小差异（0.4s）产提示不合并、3 条同时间轴的处理。
  - clean_special：OCR ` l `→` I `、`--`→`…`、连续点、多空格；表驱动可加条目。
  - normalize_cn_punct：逗号/句号/问号/感叹号全角、引号「」奇偶配对、行尾。
  - normalize_en_punct：英文侧半角、连续点、行尾补句号。
  - regularize_dialogue：已有 `-` 规整成「- 」中英对齐；跨条产提示。
  - regularize_markers：()/[]/# 格式规整；疑似未标注产提示。
  - classify_style：《》→Title、()→Note、其余 Default；有/无英文选双行/单行样式。
  - check_timeline_cross：排序 + 前后 3 条窗口检出重叠。
- **端到端**：几个代表性 .ass（同条中英、纯中、含对话 `-`、含《》片名）跑完整 pipeline，断言关键输出 + Issue 数。
- `cargo test --lib` 全绿；`tsc`/`npm run build` 通过。
- 真机：工具箱选 docs/subtitles 校准 → 输出结构正确、结果列表按文件分组显示各类提示、播放校准后字幕正常。

## 明确不做（YAGNI）

- 自动臆测「这是对话/道具/外语/歌曲」（只规整已标注 + 提示疑似）。
- 自动补跨条 `...`（只提示）。
- 还原 Java 损坏的乱码字符映射（基础表 + 用户可扩展）。
- 撤销/预览 diff（本轮直接输出到固定库；用户可从 .bak 或重跑）。
- 逐文件手动确认 UI（批量跑 + 列表提示，人工另行编辑）。

## 风险

- **规则多、易漏或误改**：pipeline 拆阶段 + 每阶段单测是主要防线；对照 Java filter/sinicized/SubtitlesSearch 逐条核（规则10）在实现计划中列成 checklist。
- **中英段边界**：所有标点/字符处理必须严格只作用于 SEPARATOR 对应的中文段或英文段，不能串（现有 normalize_text 已按 SEPARATOR 拆——保持）。中文标点误伤英文、或反之，是最可能的 bug，单测重点覆盖。
- **引号「」配对**：Java 用奇偶切换，跨行/未闭合需容错（末尾未配对提示或补全）——单测覆盖奇数引号。
- **合并误判**：只在时间轴完全相同才合并，最安全；微小差异只提示。3 条以上同时间轴的边界按「中+英两条，多余记提示」。
- **性能**：1295 个文件全量校准，pipeline 纯字符串处理，单线程可接受；如慢可后续并行（本轮不做）。
