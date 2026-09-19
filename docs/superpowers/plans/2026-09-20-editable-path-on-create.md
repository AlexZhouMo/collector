# 新增条目时可编辑目录路径 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 新增视频/漫画/游戏时提供可编辑的"目录路径"文本框（默认当前目录，可任意改，基本非法字符校验），从而把条目落到任意/新建目录；新增按钮在任何目录页都显示。

**Architecture:** 纯前端改动。新增校验纯函数 `isValidCategoryPath`；EditDrawer 新增模式渲染路径输入框（绑定到可变变量、实时校验、非法禁用保存）；MediaLibraryView 让新增按钮始终显示。"新路径自动派生新文件夹"已实现（保存→refresh→buildVideoTree 派生），不改。

**Tech Stack:** TypeScript (Vite)。

**验证：** `cd /Users/zhoumo/Documents/Claude/collector && npx tsc --noEmit && npm run build`；运行 `npm run tauri:dev` 真机验证。

**关键语义：** 路径框填"分类内目录路径"（不含分类名、不含视频文件名，如 `犯罪题材/教父`），与后端 category_path 一致，无需前缀转换。空串合法（=分类根）。

---

## Task 1: 路径校验纯函数 isValidCategoryPath

**Files:**
- Create/Modify: `src/lib/pathValidate.ts`（新建）

- [ ] **Step 1: 实现校验函数**

新建 `src/lib/pathValidate.ts`：
```typescript
/** 分类内目录路径合法性校验（基本非法字符）。
 * 规则：禁文件系统非法字符 \ : * ? " < > | 及控制字符；不以 / 开头或结尾；
 * 不含空段 //；任一段不为 . 或 ..；允许中文、空格、多层 a/b/c。
 * 空串合法（表示分类根）。 */
export function isValidCategoryPath(path: string): boolean {
  if (path === "") return true;                  // 分类根
  if (path.startsWith("/") || path.endsWith("/")) return false;
  if (/[\\:*?"<>|]/.test(path)) return false;     // 文件系统非法字符
  // eslint-disable-next-line no-control-regex
  if (/[\x00-\x1f]/.test(path)) return false;     // 控制字符
  const segs = path.split("/");
  for (const s of segs) {
    if (s === "" || s === "." || s === "..") return false; // 空段/. /..
    if (s.trim() === "") return false;            // 纯空白段
  }
  return true;
}
```

- [ ] **Step 2: 类型检查**

Run: `cd /Users/zhoumo/Documents/Claude/collector && npx tsc --noEmit`
Expected: 无输出。

- [ ] **Step 3: 逻辑自测（临时 tsx 或手工核对）**

核对：`isValidCategoryPath("")`=true、`"犯罪题材/教父"`=true、`"a/b/c"`=true、`"/x"`=false、`"x/"`=false、`"a//b"`=false、`"a:b"`=false、`"../x"`=false、`"a/./b"`=false。若有 tsx 可写临时脚本验证后删除；否则人工核对逻辑。

- [ ] **Step 4: 提交**
```bash
cd /Users/zhoumo/Documents/Claude/collector
git add src/lib/pathValidate.ts
git commit -m "feat(edit): 目录路径校验纯函数 isValidCategoryPath

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>"
```

---

## Task 2: EditDrawer 新增时加可编辑路径框 + 实时校验

**Files:**
- Modify: `src/components/EditDrawer.ts`

- [ ] **Step 1: 读现状确认改点**

Run: `sed -n '13,53p' src/components/EditDrawer.ts; sed -n '93,132p' src/components/EditDrawer.ts`
Expected: 确认 `categoryPath`（第 19 行常量 `item?.category_path ?? createPath ?? defaultCategory`）、表单 HTML（无路径框，第 33-36 是只读 video_path 显示）、保存逻辑 4 处用 categoryPath、save 按钮 `[data-b="save"]`。

- [ ] **Step 2: 新增时用可变 categoryPath + 渲染路径框**

- 把 `const categoryPath = ...` 改为 `let categoryPath = item?.category_path ?? createPath ?? ""`（新增默认 createPath，编辑用原值）。
- 表单 HTML：**仅新增时**（`!item`）在标题字段下方加：
```html
<div class="drawer-field">
  <label>目录路径</label>
  <input type="text" data-f="path" value="${esc(createPath ?? "")}" placeholder="分类内目录路径，如 犯罪题材/教父（留空=分类根）" />
  <div class="path-hint" data-d="path-hint" style="font-size:11px;color:var(--text-dim);min-height:14px"></div>
</div>
```
（编辑现有条目不渲染此框，保持原行为。）
- 导入校验：`import { isValidCategoryPath } from "../lib/pathValidate";`

- [ ] **Step 3: 路径框实时校验 + 禁用保存**

新增模式下，绑定路径框 `oninput`：
- 读值，`isValidCategoryPath(value)` 校验。
- 合法：更新 `categoryPath = value`（trim 尾部？按需——保留用户输入，仅校验），清空 hint，启用保存按钮。
- 非法：hint 显示"路径含非法字符或格式错误"，禁用保存按钮（`saveBtn.disabled = true` + 视觉禁用样式，参考项目现有 disabled 处理）。
- 初始渲染后跑一次校验（默认值 createPath 一般合法）。
- 编辑模式：不加此校验（保存按钮正常）。

- [ ] **Step 4: 保存用 categoryPath 变量**

确认保存逻辑（第 95-123 行）里 4 处 `categoryPath` 现在引用的是可变变量的最新值（新增时=路径框值，编辑时=原值）。因改成 `let` 且 oninput 更新它，保存时自然拿到最新值。核对 mediaCreate 的 categoryPath 参数用的是这个变量。

- [ ] **Step 5: 类型检查 + 构建**

Run: `npx tsc --noEmit && echo ok; npm run build 2>&1 | tail -1`
Expected: ok + 构建成功。

- [ ] **Step 6: 提交**
```bash
git add src/components/EditDrawer.ts
git commit -m "feat(edit): 新增条目时可编辑目录路径框 + 实时校验禁用保存

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>"
```

---

## Task 3: 新增按钮任何目录都显示

**Files:**
- Modify: `src/views/MediaLibraryView.ts`

- [ ] **Step 1: 读 updateAddBtn 与 onclick 现状**

Run: `sed -n '118,180p' src/views/MediaLibraryView.ts`
Expected: 确认 `addBtn.onclick = () => openEditDrawer(null, refresh, editCategory(), cfg.kind, folderPath)`、`updateAddBtn` 显示条件 `(mode === "folder" && folderPath !== "")`、tree 模式下 folderPath 是否有值。

- [ ] **Step 2: 改显示条件为始终显示**

`updateAddBtn` 里 `addBtn.style.display = (mode === "folder" && folderPath !== "") ? "" : "none";` 改为始终显示：`addBtn.style.display = "";`（folder 与 tree 模式都显示）。

- [ ] **Step 3: 确认默认路径传参**

`addBtn.onclick` 传给 EditDrawer 的 createPath：
- folder 模式：`folderPath`（当前浏览目录，可能为 ""=根）。
- tree 模式：treeSelected 或 ""（分类根）。用当前已有的 folderPath 变量即可（tree 模式 folderPath 可能为空，落分类根，合理）。
- 确认 onclick 现有传参 `folderPath` 在两种模式下都是合理的默认目录；若 tree 模式该用 treeSelected，酌情调整（但空串=分类根也可接受，保持简单）。

- [ ] **Step 4: 类型检查 + 构建**

Run: `npx tsc --noEmit && npm run build 2>&1 | tail -1`
Expected: ok + 构建成功。

- [ ] **Step 5: 提交**
```bash
git add src/views/MediaLibraryView.ts
git commit -m "feat(edit): 新增按钮在任何目录页都显示（含一级根目录、tree 模式）

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>"
```

---

## Task 4: 端到端验证

- [ ] **Step 1: 全套编译**

Run: `cd /Users/zhoumo/Documents/Claude/collector && npx tsc --noEmit && npm run build 2>&1 | tail -1`
Expected: 无类型错 + 构建成功。

- [ ] **Step 2: 真机验证**

`npm run tauri:dev`，逐一确认：
- 一级根目录页（电影/漫画/游戏）：新增按钮可见（之前隐藏）。
- 新增视频：路径框默认填当前目录；改为新路径 `测试题材/新片` → 保存 → 列表出现"测试题材"文件夹含该视频。
- 非法路径（`a//b`、`/x`、`a:b`、`../x`）→ hint 提示 + 保存按钮禁用。
- 合法路径（中文、多层、留空=分类根）→ 可保存。
- 漫画/游戏新增：同样可编辑路径、落新目录显示。

- [ ] **Step 3: 工作树确认**

Run: `git status --short && git log --oneline -5`
Expected: 工作树干净；4 个任务提交在列。
