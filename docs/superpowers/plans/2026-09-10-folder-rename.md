# 文件夹重命名（内联编辑 + 级联更新）实现计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 在文件夹网格卡片名上做内联重命名，保存时级联更新该目录及所有子目录条目的 category_path，并同步 rename 磁盘视频文件夹。

**Architecture:** 前端 `InlineRename` 通用内联编辑组件 + `FolderView` 接入；后端新增 `rename_folder` 命令（含可测纯函数 `rename_target_path`），先 rename 磁盘文件夹、再事务级联更新库 category_path。

**Tech Stack:** Tauri 2 + vanilla-ts + Vite；后端 Rust（rusqlite）。无前端测试框架（`npm run build` = `tsc && vite build`），后端用 `cargo test`，真机 `npm run tauri dev`。

**规范约定（务必遵守）:**
- `category_path` 为分类内相对路径（不含分类名），根为空串 `""`；如 `科幻`、`科幻/星战`。
- 重命名只改末段名，`category` 不变。前缀替换须精确：`category_path == oldPath` 或以 `oldPath + "/"` 开头（不误伤 `科幻小说` 这类同前缀兄弟）。
- 磁盘视频文件夹绝对路径 = `root/category_path`，root 从 `settings::get(&db, video_root_key(category))` 取（返回 `AppResult<Option<String>>`）。
- 顺序：先磁盘 rename（失败则 Err、不动库；src 不存在或 root 空则静默跳过），再事务更新库——避免「库新磁盘旧」的播放失效。
- 字幕/封面存 app_data 独立目录、与 category_path 无关，不受影响。
- 现有：`library::paths::video_root_key(category) -> &str`；`crate::error::AppError::{Db,Other}(String)`；`Db(pub Mutex<Connection>)`，`db.0.lock().unwrap()`。
- 现有 CSS 变量：`--glass` `--accent` `--accent-glow`；`.fv-name` 是文件夹/视频卡片名类。
- `showToast(message, kind?)` 来自 `src/components/Toast`。

---

### Task 1: 后端 rename_folder 命令 + 纯函数 rename_target_path

**Files:**
- Modify: `src-tauri/src/lib.rs`（新增命令 + 纯函数 + 注册 + 测试）

- [ ] **Step 1: 写失败的单元测试**

在 `src-tauri/src/lib.rs` 末尾（或其现有 `#[cfg(test)]` 区）追加测试。测试覆盖纯函数 `rename_target_path` 与级联更新逻辑函数 `rename_folder_in_db`（Step 3 实现，把「校验+DB 级联」抽成不依赖 AppHandle 的可测函数；磁盘操作单独，不单测）：

```rust
#[cfg(test)]
mod rename_folder_tests {
    use super::*;
    use crate::db::Db;

    fn seed(db: &Db, cat: &str, cpath: &str) {
        let conn = db.0.lock().unwrap();
        conn.execute(
            "INSERT INTO media (category,category_path,title) VALUES (?1,?2,?3)",
            rusqlite::params![cat, cpath, format!("t_{cpath}")],
        ).unwrap();
    }
    fn cpath_of(db: &Db, title: &str) -> String {
        let conn = db.0.lock().unwrap();
        conn.query_row("SELECT category_path FROM media WHERE title=?1",
            rusqlite::params![title], |r| r.get(0)).unwrap()
    }

    #[test]
    fn target_path_root_and_nested() {
        assert_eq!(rename_target_path("科幻", "科幻片"), "科幻片");
        assert_eq!(rename_target_path("科幻/系列", "系列2"), "科幻/系列2");
        assert_eq!(rename_target_path("a/b/c", "x"), "a/b/x");
    }

    #[test]
    fn cascade_updates_self_and_descendants_only() {
        let db = Db::open_in_memory().unwrap();
        seed(&db, "电影", "科幻");
        seed(&db, "电影", "科幻/星战");
        seed(&db, "电影", "科幻小说");   // 同前缀兄弟，不应被改
        seed(&db, "电影", "奇幻");        // 无关
        rename_folder_in_db(&db, "电影", "科幻", "科幻片").unwrap();
        assert_eq!(cpath_of(&db, "t_科幻"), "科幻片");
        assert_eq!(cpath_of(&db, "t_科幻/星战"), "科幻片/星战");
        assert_eq!(cpath_of(&db, "t_科幻小说"), "科幻小说"); // 不变
        assert_eq!(cpath_of(&db, "t_奇幻"), "奇幻");         // 不变
    }

    #[test]
    fn reject_sibling_name_conflict() {
        let db = Db::open_in_memory().unwrap();
        seed(&db, "电影", "科幻");
        seed(&db, "电影", "奇幻");
        // 把「科幻」改成已存在的同级「奇幻」→ Err
        assert!(rename_folder_in_db(&db, "电影", "科幻", "奇幻").is_err());
    }

    #[test]
    fn reject_empty_or_slash_name() {
        let db = Db::open_in_memory().unwrap();
        seed(&db, "电影", "科幻");
        assert!(rename_folder_in_db(&db, "电影", "科幻", "").is_err());
        assert!(rename_folder_in_db(&db, "电影", "科幻", "  ").is_err());
        assert!(rename_folder_in_db(&db, "电影", "科幻", "a/b").is_err());
    }
}
```

- [ ] **Step 2: 运行验证失败**

Run: `cd /Users/zhoumo/Documents/Claude/collector/src-tauri && cargo test --lib rename_folder_tests 2>&1 | tail -20`
Expected: 编译失败——`rename_target_path` / `rename_folder_in_db` 未定义。

- [ ] **Step 3: 实现纯函数 + DB 级联函数**

在 `src-tauri/src/lib.rs`（`media_update` 附近）新增：

```rust
/// 由旧相对路径与新末段名算出新相对路径：保留父前缀，替换最后一段。
/// "科幻" + "科幻片" → "科幻片"；"科幻/系列" + "系列2" → "科幻/系列2"。
fn rename_target_path(old_path: &str, new_name: &str) -> String {
    match old_path.rfind('/') {
        Some(i) => format!("{}/{}", &old_path[..i], new_name),
        None => new_name.to_string(),
    }
}

/// 校验 + 事务级联更新 category_path（不含磁盘操作，便于单测）。
/// 校验：new_name 去空非空且不含 '/'；同级(newPath)无同名文件夹。
/// 更新：category 不变；category_path == old_path → new_path；
///       category_path LIKE old_path||'/%' → 前缀替换为 new_path。
fn rename_folder_in_db(db: &Db, category: &str, old_path: &str, new_name: &str) -> AppResult<()> {
    let name = new_name.trim();
    if name.is_empty() || name.contains('/') {
        return Err(crate::error::AppError::Other("名称无效".into()));
    }
    let new_path = rename_target_path(old_path, name);
    if new_path == old_path {
        return Ok(()); // 无变化
    }
    let conn = db.0.lock().unwrap();
    // 同级重名校验：newPath 自身或其后代已存在
    let exists: i64 = conn.query_row(
        "SELECT COUNT(*) FROM media WHERE category=?1 AND (category_path=?2 OR category_path LIKE ?2 || '/%')",
        rusqlite::params![category, new_path],
        |r| r.get(0),
    ).map_err(|e| crate::error::AppError::Db(e.to_string()))?;
    if exists > 0 {
        return Err(crate::error::AppError::Other("已存在同名文件夹".into()));
    }
    // 级联更新：先后代（LIKE），再自身等值
    conn.execute(
        "UPDATE media SET category_path = ?1 || substr(category_path, length(?2)+1) \
         WHERE category=?3 AND category_path LIKE ?2 || '/%'",
        rusqlite::params![new_path, old_path, category],
    ).map_err(|e| crate::error::AppError::Db(e.to_string()))?;
    conn.execute(
        "UPDATE media SET category_path = ?1 WHERE category=?2 AND category_path = ?3",
        rusqlite::params![new_path, category, old_path],
    ).map_err(|e| crate::error::AppError::Db(e.to_string()))?;
    Ok(())
}
```

- [ ] **Step 4: 运行验证测试通过**

Run: `cd /Users/zhoumo/Documents/Claude/collector/src-tauri && cargo test --lib rename_folder_tests 2>&1 | tail -20`
Expected: 4 个测试全 ok。

- [ ] **Step 5: 加 tauri 命令（磁盘 rename + 调 DB 函数）+ 注册**

在 `src-tauri/src/lib.rs` 新增命令（放在 `media_update` 后）：

```rust
/// 重命名分类内某文件夹：先 rename 磁盘视频目录，再级联更新库 category_path。
#[tauri::command(rename_all = "camelCase")]
fn rename_folder(
    db: tauri::State<Db>,
    category: String,
    old_path: String,
    new_name: String,
) -> AppResult<()> {
    let name = new_name.trim();
    if name.is_empty() || name.contains('/') {
        return Err(crate::error::AppError::Other("名称无效".into()));
    }
    let new_path = rename_target_path(&old_path, name);
    if new_path == old_path {
        return Ok(());
    }
    // 先磁盘 rename（root 空 / src 不存在 → 跳过；rename 报错 → Err 不动库）
    let root = crate::settings::get(&db, crate::library::paths::video_root_key(&category))?
        .unwrap_or_default();
    if !root.is_empty() {
        let src = std::path::Path::new(&root).join(&old_path);
        let dst = std::path::Path::new(&root).join(&new_path);
        if src.is_dir() {
            std::fs::rename(&src, &dst)
                .map_err(|e| crate::error::AppError::Other(format!("重命名文件夹失败: {e}")))?;
        }
        // src 不存在：静默跳过（用户要求磁盘无该文件夹时不报错）
    }
    // 再更新库（含同级重名校验；理论上磁盘 dst 已成功，库校验兜底）
    rename_folder_in_db(&db, &category, &old_path, name)
}
```

在 `invoke_handler![...]` 列表中 `media_update,` 之后加一行 `rename_folder,`。

- [ ] **Step 6: 后端整体编译 + 测试通过**

Run: `cd /Users/zhoumo/Documents/Claude/collector/src-tauri && cargo test --lib 2>&1 | tail -15`
Expected: 编译成功，全部测试 ok（含新增 4 个）。

- [ ] **Step 7: 提交**

```bash
cd /Users/zhoumo/Documents/Claude/collector
git add src-tauri/src/lib.rs
git commit -m "feat(backend): rename_folder 命令(级联更新 category_path + 同步磁盘)

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>"
```

---

### Task 2: 前端 ipc 加 renameFolder

**Files:**
- Modify: `src/lib/ipc.ts`

- [ ] **Step 1: 加 API**

在 `src/lib/ipc.ts` 的 `api` 对象里（`mediaUpdate` 附近）新增：
```ts
  renameFolder: (category: string, oldPath: string, newName: string) =>
    invoke<void>("rename_folder", { category, oldPath, newName }),
```

- [ ] **Step 2: 类型检查通过**

Run: `cd /Users/zhoumo/Documents/Claude/collector && npx tsc --noEmit`
Expected: 无错误。

- [ ] **Step 3: 提交**

```bash
cd /Users/zhoumo/Documents/Claude/collector
git add src/lib/ipc.ts
git commit -m "feat: ipc 新增 renameFolder

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>"
```

---

### Task 3: InlineRename 通用内联编辑组件

**Files:**
- Create: `src/components/InlineRename.ts`

- [ ] **Step 1: 写实现**

Create `src/components/InlineRename.ts`:
```ts
/**
 * 让一个承载名字的元素支持内联重命名。
 * el: 名字元素（点击后就地替换为 input，编辑结束再恢复）；
 * current: 当前名字；
 * onCommit(newName): 确认保存回调，返回 Promise——resolve 表示保存成功（调用方通常会重建 UI），
 *   reject 表示保存失败（组件保持 input 在编辑态并重新聚焦）。
 * 交互：点击进入编辑（input 值=current、全选、聚焦）；回车或 blur 提交；Esc 取消恢复原名。
 * newName 去空后为空或等于 current → 视为取消（不调 onCommit）。
 */
export function attachInlineRename(
  el: HTMLElement,
  current: string,
  onCommit: (newName: string) => Promise<void>
): void {
  el.onclick = (e) => {
    e.stopPropagation(); // 防止触发文件夹进入等外层点击
    enterEdit();
  };

  const enterEdit = () => {
    const input = document.createElement("input");
    input.className = "fv-name-edit";
    input.value = current;
    const parent = el.parentElement;
    if (!parent) return;
    el.style.display = "none";
    parent.insertBefore(input, el.nextSibling);
    input.focus();
    input.select();

    let committing = false;
    let cancelled = false;

    const restore = () => {
      input.remove();
      el.style.display = "";
    };

    const commit = async () => {
      if (committing || cancelled) return;
      const name = input.value.trim();
      if (!name || name === current) { restore(); return; } // 空/无改动 → 取消
      committing = true;
      try {
        await onCommit(name);
        // 成功：调用方会重建 UI；此处也移除 input 兜底
        restore();
      } catch {
        // 失败：留在编辑态，重新聚焦让用户改
        committing = false;
        input.focus();
        input.select();
      }
    };

    input.addEventListener("keydown", (ev) => {
      if (ev.key === "Enter") { ev.preventDefault(); commit(); }
      else if (ev.key === "Escape") { ev.preventDefault(); cancelled = true; restore(); }
      ev.stopPropagation(); // 不冒泡给全局 Esc（如播放器）
    });
    input.addEventListener("blur", () => { if (!cancelled) commit(); });
  };
}
```

- [ ] **Step 2: 类型检查通过**

Run: `cd /Users/zhoumo/Documents/Claude/collector && npx tsc --noEmit`
Expected: 无错误。

- [ ] **Step 3: 提交**

```bash
cd /Users/zhoumo/Documents/Claude/collector
git add src/components/InlineRename.ts
git commit -m "feat: InlineRename 通用内联编辑组件(点击编辑/回车失焦保存/Esc取消)

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>"
```

---

### Task 4: FolderView 接入内联重命名 + 样式

**Files:**
- Modify: `src/components/FolderView.ts`（签名加 onRenamed、文件夹名接入 attachInlineRename）
- Modify: `src/styles/theme.css`（悬停高亮 + 编辑输入框样式）

- [ ] **Step 1: FolderView 签名加 onRenamed 回调**

在 `src/components/FolderView.ts` 顶部加 import：
```ts
import { attachInlineRename } from "./InlineRename";
import { api } from "../lib/ipc";
import { showToast } from "./Toast";
```
把 `FolderView` 函数签名末尾加一个可选参数 `onRenamed`（回传旧、新路径，供上层重映射浏览位置）：
```ts
export function FolderView(
  root: TreeNode,
  onOpen: (it: MediaItem) => void,
  onContext?: (it: MediaItem, x: number, y: number) => void,
  initialPath?: string,
  onNav?: (path: string) => void,
  onRenamed?: (oldPath: string, newPath: string) => void
): HTMLElement {
```

- [ ] **Step 2: 文件夹卡片名接入内联重命名**

在 `render()` 里，文件夹卡片渲染后绑定事件的地方（`el.querySelectorAll(".fv-folder").forEach(...)` 附近），对每个文件夹的名字元素接入内联重命名。当前文件夹卡片 HTML 是：
```
<div class="fv-cell fv-folder" data-folder="...">
  <div class="fv-folder-icon">...</div>
  <span class="fv-name">名字</span>
</div>
```
先给文件夹名 span 加可编辑标记类。把 folders 模板里的 `<span class="fv-name">${esc(c.name)}</span>` 改为：
```ts
<span class="fv-name fv-name-editable" data-folder-name="${esc(c.path)}">${esc(c.name)}</span>
```
然后在事件绑定区（`.fv-folder` 的 onclick 之后）新增：
```ts
    el.querySelectorAll<HTMLElement>(".fv-name-editable").forEach((nameEl) => {
      const path = nameEl.dataset.folderName!;
      const child = node.children.find((c) => c.path === path);
      if (!child) return;
      attachInlineRename(nameEl, child.name, async (newName) => {
        try {
          await api.renameFolder(root.name, child.path, newName);
        } catch (e) {
          showToast("重命名失败：" + e, "error");
          throw e; // 让 InlineRename 保持编辑态
        }
        showToast("已重命名");
        // 计算新 path 通知上层刷新并定位（回传旧、新路径供重映射浏览位置）
        const parent = child.path.includes("/") ? child.path.slice(0, child.path.lastIndexOf("/")) : "";
        const newPath = parent ? `${parent}/${newName}` : newName;
        onRenamed?.(child.path, newPath);
      });
    });
```
注意：`.fv-folder` 卡片本身的 onclick 是「进入文件夹」。名字 span 的点击在 `attachInlineRename` 里 `stopPropagation`，不会触发进入。但点击文件夹图标区仍进入——符合预期（点名字=改名，点图标/空白=进入）。

- [ ] **Step 3: 样式**

在 `src/styles/theme.css` 的 `.fv-name` 规则附近追加：
```css
/* 文件夹名内联编辑 */
.fv-name-editable{cursor:text;border-radius:6px;padding:2px 6px;transition:background .12s}
.fv-name-editable:hover{background:var(--glass);outline:1px dashed var(--accent-glow)}
.fv-name-edit{font:inherit;font-size:12px;text-align:center;color:#fff;background:rgba(13,18,32,.92);
  border:1px solid var(--accent);border-radius:6px;padding:2px 6px;width:100%;box-sizing:border-box;
  box-shadow:0 0 0 3px var(--accent-glow);outline:none}
```

- [ ] **Step 4: 类型检查通过**

Run: `cd /Users/zhoumo/Documents/Claude/collector && npx tsc --noEmit`
Expected: 无错误。

- [ ] **Step 5: 提交**

```bash
cd /Users/zhoumo/Documents/Claude/collector
git add src/components/FolderView.ts src/styles/theme.css
git commit -m "feat(folder): 文件夹名内联重命名(悬停高亮/点击编辑) + 样式

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>"
```

---

### Task 5: VideoView 接线刷新 + 构建验证

**Files:**
- Modify: `src/views/VideoView.ts`（给 FolderView 传 onRenamed）

- [ ] **Step 1: 传 onRenamed 回调（重映射浏览位置 + 刷新）**

在 `src/views/VideoView.ts` 的 `render()` 里，FolderView 调用处（现为）：
```ts
      ? FolderView(tree, onOpen, onContext, folderPath, (p) => { folderPath = p; })
```
改为（Task 4 已把 onRenamed 签名定为 `(oldPath, newPath)`）：
```ts
      ? FolderView(tree, onOpen, onContext, folderPath, (p) => { folderPath = p; }, (oldPath, newPath) => {
          // 当前浏览路径若等于被改名的文件夹或在其子树内，跟随重映射到新路径
          if (folderPath === oldPath) folderPath = newPath;
          else if (folderPath.startsWith(oldPath + "/")) folderPath = newPath + folderPath.slice(oldPath.length);
          refresh();
        })
```
不需要额外辅助函数，内联两行判断即可。

- [ ] **Step 2: 类型检查通过**

Run: `cd /Users/zhoumo/Documents/Claude/collector && npx tsc --noEmit`
Expected: 无错误。确认 FolderView 的 onRenamed 签名（`(oldPath, newPath) => void`，Task 4 定义）与 VideoView 传入一致。

- [ ] **Step 3: 完整构建通过**

Run: `cd /Users/zhoumo/Documents/Claude/collector && npm run build 2>&1 | tail -10`
Expected: `tsc` 与 `vite build` 均成功。

- [ ] **Step 4: 提交**

```bash
cd /Users/zhoumo/Documents/Claude/collector
git add src/views/VideoView.ts
git commit -m "feat(folder): 重命名后刷新并保留浏览位置

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>"
```

---

### Task 6: 真机验证

**Files:** 无（手动验收）

- [ ] **Step 1: 启动应用**

```bash
cd /Users/zhoumo/Documents/Claude/collector
lsof -ti:1420 | xargs kill -9 2>/dev/null; pkill -f "tauri dev" 2>/dev/null; pkill -f "target/debug/collector" 2>/dev/null; sleep 1
nohup npm run tauri dev > /tmp/collector-dev.log 2>&1 & disown
```
等窗口起来。

- [ ] **Step 2: 逐项验收**

- 进某分类文件夹网格，鼠标悬停文件夹名 → 高亮（淡背景+虚线框+文本光标）。
- 点击文件夹名 → 变输入框、文字全选、聚焦；点文件夹图标区仍能进入文件夹。
- 改名回车 → toast「已重命名」、网格刷新、名字更新；进该文件夹里的视频**仍能播放**（磁盘已同步改名）。
- 点别处（blur）→ 同样保存。
- 按 Esc → 取消，恢复原名，不保存。
- 改成同级已有的名字 → 红 toast「已存在同名文件夹」、留在编辑态。
- 清空后回车 → 视为取消，恢复原名。
- 改一个含子目录的文件夹 → 子目录内视频的路径级联更新，进子目录正常、可播放。
- 同前缀兄弟不受影响（如同级有「科幻」和「科幻小说」，改「科幻」→「科幻片」，「科幻小说」不变）。

- [ ] **Step 3: 关闭应用**

```bash
lsof -ti:1420 | xargs kill -9 2>/dev/null; pkill -f "tauri dev" 2>/dev/null
```

---

## 自审（对照 spec）

- **spec 覆盖**：内联编辑三态→Task3 InlineRename + Task4 样式；仅文件夹卡片名→Task4（视频名不加）；级联更新库→Task1 rename_folder_in_db；同步磁盘+无文件夹不报错→Task1 命令(src.is_dir 判断+跳过)；先磁盘后库→Task1 命令顺序；同级重名/空名拒绝→Task1 校验 + 前端 toast(Task4)；精确前缀不误伤→Task1 SQL(LIKE oldPath||'/%')+ 单测；刷新保留位置→Task5 onRenamed 重映射；仅视频→范围内。全部有对应任务。
- **占位符**：无 TBD/TODO，每步含完整代码。onRenamed 签名统一为 `(oldPath, newPath)`（Task4 定义、Task5 消费）。
- **类型一致**：`renameFolder(category, oldPath, newName)` 前后端一致（Task1/2）；`rename_target_path(old_path,new_name)`、`rename_folder_in_db(db,category,old_path,new_name)` 测试与实现签名一致（Task1）；`onRenamed(oldPath, newPath)` 在 FolderView 定义与 VideoView 消费一致（Task4 修订 + Task5）；`attachInlineRename(el, current, onCommit)` 定义(Task3)与消费(Task4)一致。

## 风险

- **前缀替换误伤**：用参数化 `LIKE old_path || '/%'` + 等值两条，`substr(category_path, length(old_path)+1)` 从旧前缀后接续，`科幻小说` 不被 `科幻` 命中。Task1 单测 `cascade_updates_self_and_descendants_only` 覆盖。
- **磁盘 rename 失败**：先磁盘后库，rename 报错则 Err 不动库，不产生「库新磁盘旧」。src 不存在/root 空静默跳过（用户要求）。
- **点击名字 vs 进入文件夹**：名字 span 的点击 stopPropagation，不触发卡片进入；图标区点击仍进入。真机 Step2 验证。
- **UNIQUE(category_path,title)**：同级重名前置校验拦截，避免事务中途 UNIQUE 报错。
