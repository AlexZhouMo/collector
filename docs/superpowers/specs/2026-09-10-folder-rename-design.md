# 文件夹重命名（内联编辑 + 级联更新）设计

日期：2026-09-10
状态：设计已确认，待写实现计划

## 背景与问题

视频库用 `FolderView`（`src/components/FolderView.ts`）进入式浏览目录树。文件夹名目前只读——用户无法给分类下的文件夹改名。媒体条目在数据库只存 `category_path`（分类内相对路径，不含分类名，如 `科幻/星战`），视频磁盘路径由 `root + category_path + title.mkv` 运行时推导（见 `library::paths::video_abs_path`）；字幕、封面存在 app_data 的独立目录、与 category_path 无关。

## 目标

在文件夹网格视图里，用户可对文件夹卡片名做**内联重命名**：鼠标悬停文字高亮提示可编辑，点击进入输入态（文字选中聚焦），**回车或点击别处保存**，**Esc 取消**。保存时**级联更新**该文件夹及其所有子目录下条目的 `category_path`，并**同步重命名磁盘上的真实视频文件夹**，保持库与磁盘一致、改名后仍可正常播放。

## 决策汇总

| 项 | 决策 |
|----|------|
| 数据层 | 级联替换该文件夹及所有后代 media 的 `category_path` 前缀（`category` 不变） |
| 磁盘 | **同步** rename 磁盘视频文件夹 `root/旧path` → `root/新path`；磁盘无此文件夹时**静默跳过、不报错** |
| 字幕/封面 | 不受影响（存 app_data 独立目录，与 category_path 无关） |
| 编辑入口 | **仅**文件夹网格卡片名（`.fv-name`）；面包屑保持纯导航 |
| 交互 | 悬停高亮 → 点击变输入框(文字选中+聚焦) → 回车/失焦保存、Esc 取消 |
| 冲突 | 同级已存在同名 或 空名 → 拒绝保存、toast 提示、留在编辑态 |
| 前缀匹配 | 精确：`category_path == 旧path` 或以 `旧path + "/"` 开头（不误伤同前缀兄弟如 `科幻小说`） |
| 反馈 | 成功 toast「已重命名」并刷新网格；失败 toast 错误 |
| 不做(YAGNI) | 面包屑重命名、文件夹合并、拖拽、批量重命名、漫画/游戏文件夹重命名（本次仅视频） |

## 架构

前端内联编辑 + 一个新后端命令做级联更新。数据流：

```
FolderView 卡片名 .fv-name
  → 悬停高亮；点击 → 替换为 <input>（值=当前名，全选，聚焦）
  → 回车 / blur → 校验（非空、同级不重名）→ 调 api.renameFolder(category, oldPath, newName)
  → Esc → 取消，恢复只读
  → 成功 → showToast("已重命名") + 刷新（重新 list_media 建树，回到该文件夹）
  → 失败 → showToast(错误, "error")，保持编辑态

renameFolder(category, oldPath, newName)                     [后端 Rust 新命令]
  newPath = oldPath 的父路径 + "/" + newName（根级则就是 newName）
  1. 校验：newName 非空、不含 "/"；同级无同名（DB 查）→ 否则 Err
  2. 磁盘 rename（先于库，保证不出现"库新磁盘旧"的播放失效）：
       root = settings[video_root_key(category)]；src = root/oldPath, dst = root/newPath；
       src 存在且是目录 → std::fs::rename；src 不存在或 root 空 → 静默跳过（不报错）；
       rename 报错(权限/占用) → 直接 Err，不动数据库
  3. 磁盘 OK 后，事务内级联更新 category_path（category 不变）：
       对 category_path == oldPath          → newPath
       对 category_path LIKE oldPath + "/%"  → 用 newPath 替换 oldPath 前缀
     （用参数化精确前缀，避免误伤同前缀兄弟）
  4. 返回 Ok
```

## 组件与文件

### 新增 `src/components/InlineRename.ts`

把「一个只读文字元素 → 内联编辑」的通用交互抽成一个可复用、可独立测试的小组件，隔离 DOM 编辑细节：

```ts
/**
 * 让一个元素支持内联重命名。
 * el: 承载名字的元素（点击后就地替换为 input）；
 * current: 当前名字；
 * onCommit(newName): 用户确认保存的回调（返回 Promise，reject 表示保存失败、应留在编辑态）。
 * 交互：点击进入编辑（input 值=current、全选、聚焦）；回车或 blur 提交；Esc 取消恢复。
 * 提交时 newName 与 current 相同或为空白 → 视为取消（不调 onCommit）。
 */
export function attachInlineRename(
  el: HTMLElement,
  current: string,
  onCommit: (newName: string) => Promise<void>
): void
```

要点：
- 悬停高亮由 CSS 类负责（见样式），组件只管点击/编辑/提交/取消。
- 进入编辑：创建 `<input class="fv-name-edit">`，值=current，`select()` 全选，`focus()`。
- 提交：读 `input.value.trim()`；若等于 current 或为空 → 恢复只读、不调 onCommit（视为无改动/取消）；否则 `await onCommit(newName)`，成功则组件不管刷新（由调用方重建），失败（reject）则保持 input 在编辑态并重新聚焦。
- Esc：`keydown` 捕获 Escape → 恢复只读原名，阻止冒泡（避免触发播放器等的 Esc）。
- 防重复提交：blur 与回车都会触发提交，用一个 `committing` 标志避免重复调用；Esc 取消后 blur 不再提交。

### 修改 `src/components/FolderView.ts`

- 文件夹卡片名 `.fv-name` 上：给它加类 `fv-name-editable`（供 CSS 悬停高亮），并对每个文件夹卡片名调 `attachInlineRename(nameEl, folderNode.name, onCommit)`。
- `onCommit(newName)`：调 `await api.renameFolder(activeCat, folderNode.path, newName)`；成功 → `showToast("已重命名")` + 触发上层刷新（复用 FolderView 现有的重建/刷新路径，改名后停留在当前文件夹）；失败 → `showToast("重命名失败：" + e, "error")` 并 throw（让 InlineRename 保持编辑态）。
- 视频卡片名 `.fv-name` **不加**可编辑（仅文件夹）。
- 面包屑 `.crumb` 不变（纯导航）。
- 需要 `activeCat`（分类名）传入或已可得——FolderView 目前接收的是 `root: TreeNode`，`root.name` 即分类名，可用作 category。

### 修改 `src/lib/ipc.ts`

新增 API：
```ts
renameFolder: (category: string, oldPath: string, newName: string) =>
  invoke<void>("rename_folder", { category, oldPath, newName }),
```

### 后端新增命令 `rename_folder`（`src-tauri/src/lib.rs` + 一个可测纯函数）

```rust
#[tauri::command(rename_all = "camelCase")]
fn rename_folder(
    db: tauri::State<Db>,
    category: String,
    old_path: String,   // 分类内相对路径，如 "科幻" 或 "科幻/系列"
    new_name: String,   // 仅末段新名字，不含 "/"
) -> AppResult<()>
```

逻辑：
1. **校验**：`new_name` 去空后非空、不含 `/`；否则 `Err(AppError::Other("名称无效"))`。
2. **算 newPath**：`old_path` 的父前缀 + `/` + `new_name`（`old_path` 无 `/` 即根级文件夹，newPath = new_name）。抽成纯函数 `rename_target_path(old_path, new_name) -> String` 便于单测。
3. **同级重名校验**：查 `media` 是否已存在 `category_path == newPath` 或以 `newPath + "/"` 开头的行；存在则 `Err(AppError::Other("已存在同名文件夹"))`。
4. **磁盘 rename（先于库更新）**：`root = settings.get(video_root_key(category))`；若 root 非空：`src = Path::new(root).join(old_path)`、`dst = Path::new(root).join(new_path)`。`src.is_dir()` → `std::fs::rename(src, dst)`，失败（权限/占用/目标已存在）→ `Err`，**不动数据库**；`src` 不存在或 root 空 → 静默跳过。
5. **事务级联更新**（`category` 不变）：
   - `UPDATE media SET category_path = ?newPath WHERE category = ?category AND category_path = ?oldPath`
   - `UPDATE media SET category_path = ?newPath || substr(category_path, length(?oldPath)+1) WHERE category = ?category AND category_path LIKE ?oldPath || '/%'`
   （第二条对后代做前缀替换：`科幻/星战` 在 old=`科幻`、new=`科幻片` 时 → `科幻片/星战`。用参数化 `LIKE oldPath || '/%'` 精确匹配后代，`科幻小说` 不匹配。）
6. 返回 `Ok(())`。

命令注册进 `invoke_handler`。

### 样式 `src/styles/theme.css`

```css
/* 文件夹名内联编辑 */
.fv-name-editable{cursor:text;border-radius:6px;padding:3px 6px;transition:background .12s}
.fv-name-editable:hover{background:var(--glass);outline:1px dashed var(--accent-glow)}
.fv-name-edit{font:inherit;font-size:12px;text-align:center;color:#fff;background:rgba(13,18,32,.9);
  border:1px solid var(--accent);border-radius:6px;padding:3px 6px;width:100%;box-sizing:border-box;
  box-shadow:0 0 0 3px var(--accent-glow);outline:none}
```

## 错误处理

- **空名 / 含斜杠 / 与原名相同**：前端 InlineRename 视为取消（不调后端），或后端兜底 `Err`。
- **同级重名**：后端第 3 步 `Err("已存在同名文件夹")` → 前端 toast、保持编辑态。
- **磁盘无该文件夹**：静默跳过 rename，库更新照常成功（用户明确要求不报错）。此时库与磁盘本就无对应关系，改名只影响库内组织。
- **磁盘 rename 真失败（权限/占用/目标已存在）**：库更新已在事务中做。为避免「库改了、磁盘没改、播放失效」，采用**先磁盘后库**或**磁盘失败则回滚库**——见下「一致性」。
- **root 未配置（空）**：跳过磁盘操作，仅更新库（与「磁盘无文件夹」同等处理，不报错）。

### 一致性（磁盘与库的顺序）

为避免库改成功但磁盘失败导致播放失效，命令内顺序：
1. 先做校验（含同级重名）。
2. **先尝试磁盘 rename**（若 src 是目录）：成功或「src 不存在（跳过）」都继续；若 rename 报错（权限/目标占用）→ 直接 `Err`，**不动数据库**。
3. 磁盘 OK 后，再在事务内更新数据库 category_path。
这样：磁盘失败 → 库不变、报错；磁盘成功或本就无磁盘目录 → 库更新。保证不出现「库新、磁盘旧」的播放失效态。

## 测试策略

无前端测试框架。

- **纯函数单测**（后端 `cargo test`）：
  - `rename_target_path("科幻", "科幻片")` → `"科幻片"`；`rename_target_path("科幻/系列", "系列2")` → `"科幻/系列2"`；`rename_target_path("", ...)` 边界。
  - 级联更新 SQL 逻辑：内存库插入 `科幻`、`科幻/星战`、`科幻小说`（同前缀兄弟）、`奇幻` 若干条，调用重命名 `科幻`→`科幻片`，断言：`科幻`→`科幻片`、`科幻/星战`→`科幻片/星战`、`科幻小说` **不变**、`奇幻` 不变。同级重名（已存在 `科幻片`）断言 `Err`。
- **前端**：`npx tsc --noEmit`、`npm run build` 通过。
- **真机**（`npm run tauri dev`）：
  1. 进某文件夹网格，悬停文件夹名有高亮；点击变输入框、文字选中。
  2. 改名回车 → toast「已重命名」、网格刷新、名字更新；进入该文件夹里的视频仍能播放（磁盘已同步改名）。
  3. 点别处（blur）也保存；按 Esc 取消恢复原名。
  4. 改成同级已有的名字 → 红 toast「已存在同名文件夹」、留在编辑态。
  5. 清空回车 → 视为取消，恢复原名。
  6. 改一个有子目录的文件夹 → 子目录下视频的 category_path 级联更新，进子目录仍正常。
  7. （容错）某文件夹在磁盘上不存在时改名 → 不报错，库内改名成功。

## 明确不做（YAGNI）

- 面包屑上重命名。
- 文件夹合并（重名不合并，直接拒绝）。
- 拖拽移动/排序、批量重命名。
- 漫画 / 游戏分类的文件夹重命名（本次仅视频；结构不同，另行设计）。
- 撤销/重做。

## 风险

- **前缀替换误伤**：必须用参数化 `category_path LIKE oldPath || '/%'` + 精确等值两条，且 `substr` 从 `length(oldPath)+1` 起替换，确保 `科幻小说` 不被 `科幻` 命中。已用单测覆盖。
- **磁盘 rename 的跨设备/权限**：`std::fs::rename` 跨设备会失败——本场景同一 root 下同层改名，不跨设备，风险低；失败则报错且不动库（一致性策略）。
- **UNIQUE(category_path,title) 冲突**：同级重名会导致后代路径撞已存在条目——第 3 步同级重名校验在事务前拦截，避免事务中途 UNIQUE 报错。
- **刷新后停留位置**：改名后当前文件夹的 path 变了，刷新需定位到 newPath（FolderView 的 initialPath/onNav 机制已支持按 path 回定位，传 newPath 即可）。
