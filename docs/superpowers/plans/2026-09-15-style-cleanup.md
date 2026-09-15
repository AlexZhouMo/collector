# 样式文件清理 实现计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 删除样式文件中的死代码与冗余，统一 `.add-video-btn` 命名为 `.add-btn`，视觉与行为零变化。

**Architecture:** 纯删除/等价合并/改名。图片资源不动（无孤儿）。前端无单测，靠 `tsc --noEmit` + 应用手动验证兜底。删除会使行号漂移，故实现时用**内容匹配**定位，不依赖行号。

**Tech Stack:** CSS、TypeScript（vanilla-ts）。工作在 master 直接进行。

---

## Task 1: 删除死 CSS（A 组）

**Files:**
- Modify: `src/styles/theme.css`
- Modify: `src/styles/animations.css`

- [ ] **Step 1: 删除遗留单页/条带阅读器块（theme.css）**

删除这四条相邻规则（内容匹配定位）：

```css
.stage.page{display:flex;align-items:center;justify-content:center}
.page-img{max-height:calc(100vh - 120px);border-radius:8px;animation:fadeInUp .2s ease}
.stage.strip{display:block;overflow:auto}
.strip-img{display:block;margin:0 auto;max-width:100%}
```

（以上为该块的语义；实际文本以文件为准——定位 `.stage.page`、`.page-img`、`.stage.strip`、`.strip-img` 四行并整删。若某行属性与此处不完全一致，以文件实际内容为准删除对应整行。）

- [ ] **Step 2: 删除游戏卡片样式块（theme.css）**

删除这 8 条 `.game-*` 规则（定位 `.game-grid` 到 `.game-card.disabled` 的连续块，整删）：`.game-grid`、`.game-card`、`.game-cover`、`.game-cover img`、`.game-title`、`.game-desc`、`.game-na`、`.game-card.disabled`。

- [ ] **Step 3: 删除死的 leaf-turn 机制（theme.css）**

删除 `.book .leaf.turn-in{...}` 规则整行，以及紧随其后的 `@keyframes leafTurn{...}` 整块。
**保留** `.book .leaf`、`.book .leaf.left`、`.book .leaf.right`、`.book.single .leaf`（这些仍在用），只删 `.turn-in` 变体与 `leafTurn` 关键帧。

- [ ] **Step 4: 删除未用动画（animations.css）**

删除：

```css
.card-hover { transition: transform .18s, box-shadow .18s; }
.card-hover:hover { transform: translateY(-4px); box-shadow: 0 12px 30px rgba(0,0,0,.5); }
```

（以 `.card-hover` 与 `.card-hover:hover` 两条为准，整删。）

- [ ] **Step 5: 确认删除彻底 + fadeInUp 保留**

Run: `cd /Users/zhoumo/Documents/Claude/collector && grep -nE "\.stage\.page|\.page-img|\.stage\.strip|\.strip-img|\.game-grid|\.game-card|\.game-cover|\.game-title|\.game-desc|\.game-na|leaf\.turn-in|leafTurn|card-hover" src/styles/*.css || echo "全部删除干净"`
Expected: 输出 "全部删除干净"（无残留）。

Run: `grep -n "fadeInUp\|view-enter" src/styles/animations.css`
Expected: `@keyframes fadeInUp` 与 `.view-enter{animation:fadeInUp...}` 仍在（未被误删）。

- [ ] **Step 6: 类型检查（形式性）**

Run: `cd /Users/zhoumo/Documents/Claude/collector && npx tsc --noEmit 2>&1 | tail -5`
Expected: 无错误。

- [ ] **Step 7: 提交**

```bash
git add src/styles/theme.css src/styles/animations.css
git commit -m "refactor(css): 删除死规则（单页阅读器/游戏卡片/leaf-turn/card-hover）"
```

---

## Task 2: 去除冗余 CSS（B 组）

**Files:**
- Modify: `src/styles/theme.css`

- [ ] **Step 1: 删除字节级重复的 `.fv-video .poster-img`**

删除这两行（与 `.poster-img` / `.poster-img img` 完全重复）：

```css
.fv-video .poster-img{aspect-ratio:2/3;border-radius:10px;overflow:hidden;background:var(--glass);border:1px solid var(--border);display:flex;align-items:center;justify-content:center}
.fv-video .poster-img img{width:100%;height:100%;object-fit:cover}
```

**保留** `.fv-video.disabled .poster-img`、`.fv-video.disabled .poster-img img`、`.fv-video.disabled:hover .poster-img`（不同选择器，不受影响）。

- [ ] **Step 2: 合并 `.sidebar` 两处定义**

文件中 `.sidebar` 定义了两处：
- 第一处（布局）：`.sidebar{width:180px;height:100%;padding:16px 12px;display:flex;flex-direction:column;gap:6px}`
- 第二处（过渡）：`.sidebar{transition:width .22s ease, padding .22s ease, opacity .18s ease; overflow:hidden}`

把第二处的属性并入第一处，删除第二处那条独立 `.sidebar{...}` 规则。合并后第一处变为：

```css
.sidebar{width:180px;height:100%;padding:16px 12px;display:flex;flex-direction:column;gap:6px;transition:width .22s ease, padding .22s ease, opacity .18s ease;overflow:hidden}
```

（以文件实际属性为准合并；两处属性不重叠，合并后等价。）

- [ ] **Step 2b: 合并 `.brand` 两处定义**

- 第一处：`.brand{color:var(--text);letter-spacing:2px;font-size:13px;margin-bottom:14px;padding-left:8px}`
- 第二处：`.brand{display:flex;align-items:center;justify-content:space-between}`

把第二处属性并入第一处，删除第二处。合并后：

```css
.brand{color:var(--text);letter-spacing:2px;font-size:13px;margin-bottom:14px;padding-left:8px;display:flex;align-items:center;justify-content:space-between}
```

- [ ] **Step 2c: 合并 `.nav-item` 两处定义**

- 第一处：`.nav-item{padding:9px 12px;border-radius:10px;color:var(--text-dim);cursor:pointer;font-size:13px;transition:background .15s}`
- 第二处：`.nav-item{display:flex;align-items:center;gap:10px}`（以文件实际为准）

把第二处属性并入第一处，删除第二处。合并后包含两组属性。
**保留** `.nav-item:hover`、`.nav-item.active`（衍生状态规则，不动）。

- [ ] **Step 3: 确认各选择器只剩一处定义**

Run: `cd /Users/zhoumo/Documents/Claude/collector && for s in "^\.sidebar{" "^\.brand{" "^\.nav-item{" "\.fv-video \.poster-img{"; do echo "== $s =="; grep -nE "$s" src/styles/theme.css; done`
Expected: `.sidebar{`、`.brand{`、`.nav-item{` 各仅 1 处；`.fv-video .poster-img{` 0 处（已删）。`.nav-item:hover`/`.nav-item.active` 不计入（带伪类/组合）。

- [ ] **Step 4: 类型检查**

Run: `cd /Users/zhoumo/Documents/Claude/collector && npx tsc --noEmit 2>&1 | tail -5`
Expected: 无错误。

- [ ] **Step 5: 提交**

```bash
git add src/styles/theme.css
git commit -m "refactor(css): 去除重复的 .fv-video .poster-img，合并 sidebar/brand/nav-item 分散定义"
```

---

## Task 3: 命名统一 `.add-video-btn` → `.add-btn`（C 组）

**Files:**
- Modify: `src/views/VideoView.ts`
- Modify: `src/views/ComicView.ts`
- Modify: `src/views/GameView.ts`

- [ ] **Step 1: 三视图中替换 add-video-btn → add-btn**

在每个文件中，把 `add-video-btn` 全部替换为 `add-btn`。每个文件两处：
- markup 里的 `class="add-video-btn"` → `class="add-btn"`
- `el.querySelector<HTMLButtonElement>(".add-video-btn")!` → `".add-btn"`

三个文件：`src/views/VideoView.ts`、`src/views/ComicView.ts`、`src/views/GameView.ts`，共 6 处。

- [ ] **Step 2: 确认无残留 + CSS 无需改**

Run: `cd /Users/zhoumo/Documents/Claude/collector && grep -rn "add-video-btn" src/ || echo "无残留"`
Expected: "无残留"。

Run: `grep -rn "add-video-btn\|add-btn" src/styles/*.css || echo "CSS 无该类定义（预期）"`
Expected: CSS 中无 `.add-video-btn` 也无 `.add-btn` 定义（该按钮无专属 CSS，符合预期，无需改 CSS）。

- [ ] **Step 3: 类型检查**

Run: `cd /Users/zhoumo/Documents/Claude/collector && npx tsc --noEmit 2>&1 | tail -5`
Expected: 无错误。

- [ ] **Step 4: 提交**

```bash
git add src/views/VideoView.ts src/views/ComicView.ts src/views/GameView.ts
git commit -m "refactor(fe): 新增按钮类名 add-video-btn 统一为中性名 add-btn"
```

---

## Task 4: 整体验证

**Files:** 无（验证任务）

- [ ] **Step 1: 全量类型检查**

Run: `cd /Users/zhoumo/Documents/Claude/collector && npx tsc --noEmit 2>&1 | tail -5`
Expected: 无错误。

- [ ] **Step 2: 手动验证清单（运行应用中确认视觉与清理前一致）**

- 主页：背景、统计卡、版本分割线正常。
- 侧边栏：折叠/展开动画、品牌区、导航项 hover/active 正常（涉及 B 组 sidebar/brand/nav-item 合并）。
- 影视/漫画/游戏列表：封面卡片（`.poster-img` 渲染）、文件夹、disabled 灰显正常（涉及 B 组 poster-img 去重）。
- 三视图「+ 新增」按钮：外观、显隐、点击正常（涉及 C 组改名）。
- 播放器翻页：flipper 动画正常（涉及 A 组删 leaf-turn，确认未误伤 flipper）。

- [ ] **Step 3: 前序任务已各自提交，本任务无额外提交**

---

## 自审记录

- **Spec 覆盖**：A→Task1（四组死规则）；B→Task2（poster-img 去重 + 三选择器合并）；C→Task3（改名）；验证→Task4。全覆盖。
- **行号漂移防护**：所有删除/合并步骤用内容匹配定位，不依赖行号（删除会使后续行号变化）。
- **保留项明确**：`fadeInUp`（Task1 Step5 验证保留）、`.leaf` 系列非 turn-in 变体、`.fv-video.disabled` 衍生规则、`.nav-item:hover`/`.active`、`.libassjs-canvas-parent`——均在步骤中标注保留。
- **无占位符**：每步含确切内容、grep 验证命令与预期。CSS 属性文本以「文件实际为准」标注，因实现者会读文件确认——这是防止我记错某属性细节的保险，非占位。
