# 字幕样式四分类 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 字幕批量校准的输出样式由 `Default/Title/Note` 改为 `Bilingual/Unilingual/Title/Note` 四类，并把归类有疑问的行作为"样式存疑"提示列出。

**Architecture:** 分类与文字规整是纯字符串函数，全部放在 `classify.rs`；`subtitle.rs` 的 `format_ass` 只负责按顺序调用并把存疑原因转成 `Issue`。判断一律基于去掉 `{...}` 标签后的可见文字，改写时不碰标签内容。

**Tech Stack:** Rust（Tauri 后端，`cargo test --lib normalize`，Windows 上须先加载 MSVC 环境，见 `scripts/deploy.ps1` 的 `Import-MsvcEnv`）；前端 TypeScript（`npx tsc --noEmit`）。

**Spec:** `docs/superpowers/specs/2026-11-19-subtitle-style-types-design.md`

## Global Constraints

- 分类顺序命中即止：有英文 → `Bilingual`；整行一对 `《…》` → `Title`；整行一对 `(…)`/`[…]` → `Note`；其余单行 → `Unilingual`。
- "完整包裹" = 开头括号与结尾括号是同一对；同族嵌套不影响；多段拼接不算。
- 括号族：圆 `( （` / `) ）`；方 `[ ［ 【` / `] ］ 】`；书名号 `《` / `》`。同族内半角全角等价。
- 样式参数：`Bilingual` = 原 `Default`（MarginV 10）；`Unilingual` = `Note`（MarginV 40）；`Title`、`Note` 不变。输出不得出现 `Default`。
- 提示标签原文：`样式存疑(双语)`、`样式存疑(多段)`、`样式存疑(未闭合)`。
- 不提交（用户未要求提交）；不改播放器、seed 数据、更新日志。
- 测试编译后用 `npm run clean:win` 清理 `target\debug`。

## Review Focus

- 行首带特效标签（`{\an8}《X》`）：仍按可见文字归 `Title`，标签原样保留，标签内的空格/冒号不被规整。→ Task 1
- 分隔符后英文为空（`中文\N{\fnArial\fs30}`）：按单行处理，不得归 `Bilingual`。→ Task 2
- 半角全角混用（`（X)`、`【X]`）：算完整包裹，输出统一成英文括号。→ Task 1
- 空串、只有一个括号、空括号（``、`(`、`《》`）：不 panic，`(` 报未闭合。→ Task 1
- 外层括号内含书名号（`(正在播放 《某歌》)`）：按外层归 `Note`，不是 `Title`。→ Task 1

---

### Task 1: 分类与规整（`classify.rs`）

**Files:**
- Modify: `src-tauri/src/normalize/classify.rs`（整体重写，删除 `has_language_tag`/`is_language_tag`/`NON_LANGUAGE_TAGS` 及其测试）

**Interfaces:**
- Consumes: `crate::normalize::subtitle::strip_override_tags(&str) -> String`
- Produces:
  - `enum Style { Bilingual, Unilingual, Title, Note }`，`Style::name() -> &'static str`
  - `enum Doubt { BilingualWrapped, MultiSegment, Unclosed }`，`Doubt::label() -> &'static str`（返回上面三个标签原文）
  - `struct Classified { pub style: Style, pub doubt: Option<Doubt> }`
  - `fn classify(zh: &str, has_en: bool) -> Classified`
  - `fn strip_space_before_title_mark(s: &str) -> String`（删紧贴 `《` 前的半角/全角空格、制表符）
  - `fn normalize_title(s: &str) -> String`（标签外：删全部空白，`:`/`∶` → `：`）
  - `fn normalize_note(s: &str) -> String`（标签外：`（）→()`，`【】［］→[]`）

- [ ] **Step 1: 写失败的测试**（`classify.rs` 的 `tests` 模块，断言 `(style, doubt)`）

| 输入 zh | has_en | 期望 |
|---|---|---|
| `《肖申克的救赎》`、`{\an8}《X》`、`《》` | false | Title, None |
| `《X》后面`、`前面《X》`、`看过《圣经》`、`普通台词`、`` | false | Unilingual, None |
| `（一把钥匙）`、`(a)`、`[第1回]`、`【突发新闻】`、`（X)`、`(电话(626) 584)`、`(正在播放 《某歌》)` | false | Note, None |
| `[外语] 早上好`、`【警告】请勿靠近` | false | Unilingual, None |
| `(枪声)(枪声)`、`（a） （b）`、`《A》 《B》`、`(a)[b]` | false | Unilingual, MultiSegment |
| `（两天没换的衬衫`、`（贝克街） 221号B`、`(`、`[未闭合`、`《未闭合` | false | Unilingual, Unclosed |
| `你好`、`[外语] 早上好`、`看过《圣经》` | true | Bilingual, None |
| `《小红帽》`、`（追踪咒）`、`[咒语]` | true | Bilingual, BilingualWrapped |

规整函数：
- `strip_space_before_title_mark("是 《奇异恩典》啊") == "是《奇异恩典》啊"`；`"《A》 　《B》" → "《A》《B》"`；`"a b" → "a b"`。
- `normalize_title("《斯巴达克斯: 血与沙》") == "《斯巴达克斯：血与沙》"`；`"《圣斗士星矢　天界篇 序章》" → "《圣斗士星矢天界篇序章》"`；`"{\\fnMicrosoft YaHei}《A B》" → "{\\fnMicrosoft YaHei}《AB》"`。
- `normalize_note("（电话（626）584）") == "(电话(626)584)"`；`"【砰】" → "[砰]"`；`"{\\pos（1，2）}［X］" → "{\\pos（1，2）}[X]"`。

- [ ] **Step 2: 跑测试确认失败**（`cargo test --lib normalize::classify`，预期编译失败：类型未定义）
- [ ] **Step 3: 实现**。包裹判定：按首字符定括号族，同族计深度找到首段闭合处；逐段消耗整行，恰 1 段 = 完整包裹，≥2 段（段间只有空白，可跨族）= 多段，否则看首段是否闭合。未闭合提示：圆括号开头而非完整/多段；或方括号/书名号开头且首段从未闭合。
- [ ] **Step 4: 跑测试确认通过**

### Task 2: 接入校准管线（`subtitle.rs`）

**Files:**
- Modify: `src-tauri/src/normalize/subtitle.rs`（`build_header`、`format_ass`、相关测试）

**Interfaces:**
- Consumes: Task 1 的全部 Produces。

- [ ] **Step 1: 写失败的测试**（替换 `cleanup_tests::language_tag_rows_use_note_style`，并把 `build_tests` 里的 `Style: Default,SimHei,32` 改为 `Style: Bilingual,SimHei,32`）
  - `style_types_in_pipeline`：一份含 `你好\N{\fnArial\fs30}Hi`、`普通台词`、`《斯巴达克斯: 血与沙》`、`（一把钥匙）`、`是 《奇异恩典》啊`、`[外语] 早上好`、`中文\N{\fnArial\fs30}`（空英文）的输入 → 各行样式依次为 Bilingual、Unilingual、Title、Note、Unilingual、Unilingual、Unilingual；输出含 `《斯巴达克斯：血与沙》`、`,Note,0,0,0,0,,(一把钥匙)`、`是《奇异恩典》啊`；整个输出不含 `Default`；头部含四条 `Style:`，`Unilingual` 行除名字外与 `Note` 行相同。
  - `style_doubts_reported`：`《小红帽》\N{\fnArial\fs30}Snow White?`、`(枪声)(枪声)`、`（两天没换的衬衫` → 依次产生 `样式存疑(双语)`、`样式存疑(多段)`、`样式存疑(未闭合)`，且 `普通台词` 不产生任何"样式存疑"。
- [ ] **Step 2: 跑测试确认失败**
- [ ] **Step 3: 实现**。`format_ass` 内顺序：现有中文段规整 → `strip_space_before_title_mark` → `classify(&zh, 英文非空)` → 按样式 `normalize_title`/`normalize_note` → `rebuild` → 写行；`doubt` 转成 `Issue`（`text` 用最终文本）。`build_header` 写四条样式。
- [ ] **Step 4: 跑 `cargo test --lib normalize` 确认全部通过**

### Task 3: 前端色标、README、全库验证

**Files:**
- Modify: `src/lib/normalizeStore.ts`（`kindColor`：含"存疑"的类型给一个与现有色标不同的颜色）
- Modify: `README.md`（"自动执行的规则"第 8、9 条；"仅提示"列表加"样式存疑"三种）

- [ ] **Step 1:** 改 `kindColor` 与 README。
- [ ] **Step 2:** `npx tsc --noEmit`，预期退出码 0。
- [ ] **Step 3:** 临时测试把 `docs/subtitles` 全库跑到临时目录，统计各样式行数与"样式存疑"条数：输出无 `Default`；`Bilingual` 行全部含分隔符、其余样式行全部不含；数量与 Spec 估计（双语约 400、多段约 20、未闭合约 40）同量级。跑完删除临时测试与临时输出。
- [ ] **Step 4:** `npm run clean:win`，`git status` 只应出现本计划涉及的文件。
