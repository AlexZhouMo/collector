# 视频字幕渲染功能实现计划（JASSUB / libass-wasm）

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 播放视频时用 JASSUB（libass-wasm）完整还原渲染该条视频的 `.ass` 字幕并与时间轴同步，播放条加字幕开关。

**Architecture:** 后端 `player_open` 增加返回字幕绝对路径；前端新增 `SubtitleRenderer` 组件封装 JASSUB，`PlayerView` 用 `convertFileSrc` 把字幕 URL 交给它，CC 按钮控制显隐；打包 Noto Sans CJK 作兜底字体。时间轴同步由 JASSUB 内部跟随 `<video>` 完成。

**Tech Stack:** Tauri 2 + vanilla-ts + Vite；后端 Rust（rusqlite）；字幕引擎 jassub 2.5.16（MIT）；兜底字体 Noto Sans CJK SC（SIL OFL）。无前端测试框架（`npm run build` = `tsc && vite build`），后端用 `cargo test`，真机跑 `npm run tauri dev`。

**规范约定（务必遵守）:**
- `subtitle_path` 存相对 app_data 的路径（如 `subtitles/sub_xxx.ass`）；`media` 表主键唯一约束 `UNIQUE(category_path, title)`。
- 后端已有 `library::paths::appdata_to_absolute(rel, app_data) -> String`（rel 空→空串；已是绝对→原样）。
- `player_open` 已持有 `app: AppHandle`、`db: State<Db>`、`state`、`http` 四个参数，`#[tauri::command(rename_all = "camelCase")]`。
- 图标库 `src/lib/icons.ts` 为 Lucide 风格（24×24 viewBox、stroke currentColor、stroke-width 2），`icon(name, size=18)`。
- 前端读 app_data 下文件用 `convertFileSrc`（`@tauri-apps/api/core`），`assetProtocol.enable=true` scope `**` 已允许。
- CSP 为 `null`（宽松），不拦截 WASM/worker。

---

### Task 1: 安装 jassub 依赖 + 放置兜底字体

**Files:**
- Modify: `package.json`（新增 dependency）
- Create: `src/assets/fonts/NotoSansCJKsc-Regular.woff2`（兜底字体）
- Create: `src/assets/fonts/README.md`（字体来源与许可说明）

- [ ] **Step 1: 安装 jassub**

Run:
```bash
cd /Users/zhoumo/Documents/Claude/collector && npm install jassub@2.5.16
```
Expected: `package.json` dependencies 出现 `"jassub"`，`node_modules/jassub/dist/` 下有 `jassub-worker.js`、`jassub-worker.wasm`、`jassub-worker-modern.wasm`。

- [ ] **Step 2: 验证 jassub 产物存在**

Run:
```bash
ls node_modules/jassub/dist/jassub-worker.js node_modules/jassub/dist/jassub-worker.wasm node_modules/jassub/dist/jassub-worker-modern.wasm
```
Expected: 三个文件都列出，无 "No such file"。

- [ ] **Step 3: 下载 Noto Sans CJK SC 兜底字体（woff2）**

用思源黑体简体子集/全量 woff2。优先用 fontsource 的 CDN 产物（OFL 许可）。Run:
```bash
cd /Users/zhoumo/Documents/Claude/collector
mkdir -p src/assets/fonts
# fontsource 提供 Noto Sans SC 的 woff2（可覆盖常用简体中文）
curl -fL -o src/assets/fonts/NotoSansCJKsc-Regular.woff2 \
  "https://cdn.jsdelivr.net/fontsource/fonts/noto-sans-sc@latest/chinese-simplified-400-normal.woff2"
ls -la src/assets/fonts/NotoSansCJKsc-Regular.woff2
```
Expected: 文件存在且大小 > 100KB（真字体，非错误页）。若该 URL 失效，改用 `npm view @fontsource/noto-sans-sc` 安装后从 `node_modules/@fontsource/noto-sans-sc/files/` 拷一个 `chinese-simplified-400-normal.woff2` 到该路径。**务必确认下载的是二进制字体文件而非 HTML 错误页**（用 `file src/assets/fonts/NotoSansCJKsc-Regular.woff2` 应显示 "Web Open Font Format"）。

- [ ] **Step 4: 验证字体是真字体文件**

Run:
```bash
file /Users/zhoumo/Documents/Claude/collector/src/assets/fonts/NotoSansCJKsc-Regular.woff2
```
Expected: 输出含 "Web Open Font Format (Version 2)" 或 "WOFF2"。若显示 HTML/ASCII text 说明下载失败，回到 Step 3 换源。

- [ ] **Step 5: 写字体来源说明**

Create `src/assets/fonts/README.md`:
```markdown
# 打包字体

- `NotoSansCJKsc-Regular.woff2` — Noto Sans SC（思源黑体简体），SIL Open Font License 1.1。
  作为视频字幕渲染（JASSUB/libass）的 CJK 兜底字体（`defaultFont`），
  保证字幕中文在 macOS / Windows 上都不会因缺字体而显示为方框。
  来源：https://fontsource.org/fonts/noto-sans-sc
```

- [ ] **Step 6: 提交**

```bash
cd /Users/zhoumo/Documents/Claude/collector
git add package.json package-lock.json src/assets/fonts/
git commit -m "chore: 引入 jassub 字幕引擎 + 打包 Noto Sans CJK 兜底字体

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>"
```

---

### Task 2: 新增 captions 图标

**Files:**
- Modify: `src/lib/icons.ts`（`IconName` 类型 + `PATHS`）

- [ ] **Step 1: 加入 captions 图标**

在 `src/lib/icons.ts` 的 `IconName` 联合类型末尾加 `"captions"`：把
```ts
  | "refresh" | "arrowLeft" | "trash";
```
改为
```ts
  | "refresh" | "arrowLeft" | "trash" | "captions";
```

在 `PATHS` 对象中 `trash:` 那一行之后新增一行（Lucide captions：圆角矩形框 + 内部两行短横线）：
```ts
  captions: `<rect x="3" y="5" width="18" height="14" rx="2"/><path d="M7 15h4M15 15h2M7 11h2M13 11h4"/>`,
```

- [ ] **Step 2: 类型检查通过**

Run: `cd /Users/zhoumo/Documents/Claude/collector && npx tsc --noEmit`
Expected: 无错误（退出码 0）。

- [ ] **Step 3: 提交**

```bash
cd /Users/zhoumo/Documents/Claude/collector
git add src/lib/icons.ts
git commit -m "feat: icons 新增 captions 字幕图标(Lucide 风格)

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>"
```

---

### Task 3: 后端 player_open 返回字幕绝对路径

**Files:**
- Modify: `src-tauri/src/player/mod.rs`（`PlayerInfo` 结构 + `player_open` 查库 + 单元测试）

- [ ] **Step 1: 写失败的单元测试**

在 `src-tauri/src/player/mod.rs` 末尾追加测试模块。测试目标是「按 category_path+title 查 subtitle_path 并转绝对路径」这一纯逻辑，故把该逻辑抽成一个可测辅助函数 `resolve_subtitle`（Step 3 实现）：

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Db;

    fn seed(db: &Db, cpath: &str, title: &str, sub: Option<&str>) {
        let conn = db.0.lock().unwrap();
        conn.execute(
            "INSERT INTO media (category,category_path,title,subtitle_path) VALUES ('电影',?1,?2,?3)",
            rusqlite::params![cpath, title, sub],
        ).unwrap();
    }

    #[test]
    fn resolve_subtitle_found_relative_to_abs() {
        let db = Db::open_in_memory().unwrap();
        seed(&db, "科幻/星战", "星战", Some("subtitles/sub_a.ass"));
        let got = resolve_subtitle(&db, "科幻/星战", "星战", "/app").unwrap();
        assert_eq!(got, Some("/app/subtitles/sub_a.ass".to_string()));
    }

    #[test]
    fn resolve_subtitle_null_or_empty_is_none() {
        let db = Db::open_in_memory().unwrap();
        seed(&db, "科幻/沙丘", "沙丘", None);
        seed(&db, "科幻/降临", "降临", Some(""));
        assert_eq!(resolve_subtitle(&db, "科幻/沙丘", "沙丘", "/app").unwrap(), None);
        assert_eq!(resolve_subtitle(&db, "科幻/降临", "降临", "/app").unwrap(), None);
    }

    #[test]
    fn resolve_subtitle_missing_row_is_none() {
        let db = Db::open_in_memory().unwrap();
        assert_eq!(resolve_subtitle(&db, "不存在", "无此片", "/app").unwrap(), None);
    }
}
```

- [ ] **Step 2: 运行验证失败**

Run: `cd /Users/zhoumo/Documents/Claude/collector/src-tauri && cargo test --lib player::tests 2>&1 | tail -20`
Expected: 编译失败——`resolve_subtitle` 未定义（cannot find function）。

- [ ] **Step 3: 实现 resolve_subtitle 并在 PlayerInfo 加字段**

在 `src-tauri/src/player/mod.rs`：

(a) `PlayerInfo` 结构加 `subtitle` 字段：
```rust
#[derive(Serialize)]
pub struct PlayerInfo {
    pub src: String,
    pub duration: f64,
    pub subtitle: Option<String>,
}
```

(b) 新增辅助函数（放在 `player_open` 之前）：
```rust
/// 按 (category_path, title) 查该条视频的字幕相对路径，转成 app_data 下绝对路径。
/// 查不到行、字段为 NULL 或空串 → Ok(None)。
fn resolve_subtitle(
    db: &crate::db::Db,
    category_path: &str,
    title: &str,
    app_data: &str,
) -> AppResult<Option<String>> {
    let conn = db.0.lock().unwrap();
    let rel: Option<String> = conn
        .query_row(
            "SELECT subtitle_path FROM media WHERE category_path=?1 AND title=?2",
            rusqlite::params![category_path, title],
            |r| r.get(0),
        )
        .optional()
        .map_err(|e| crate::error::AppError::Db(e.to_string()))?
        .flatten();
    let abs = match rel {
        Some(s) if !s.trim().is_empty() => {
            Some(crate::library::paths::appdata_to_absolute(&s, app_data))
        }
        _ => None,
    };
    Ok(abs)
}
```
文件顶部 use 区确保有 `use rusqlite::OptionalExtension;`（`.optional()` 需要）。若已 `use rusqlite;` 则改为在函数内用 `rusqlite::OptionalExtension` 亦可——为稳妥，在 `mod.rs` 顶部加：
```rust
use rusqlite::OptionalExtension;
```

- [ ] **Step 4: 运行验证测试通过**

Run: `cd /Users/zhoumo/Documents/Claude/collector/src-tauri && cargo test --lib player::tests 2>&1 | tail -20`
Expected: 3 个测试全部 `ok`。

- [ ] **Step 5: 在 player_open 中填充 subtitle 字段**

在 `player_open` 函数里，构造 `PlayerInfo` 之前，取 app_data 并解析字幕；`app_data_dir` 已在函数内用于 `cache_dir`，复用其字符串形式。把结尾的返回改为：
```rust
    let app_data_str = app
        .path()
        .app_data_dir()
        .map_err(|e| crate::error::AppError::Other(format!("app_data_dir: {e}")))?
        .to_string_lossy()
        .to_string();
    let subtitle = resolve_subtitle(&db, &category_path, &title, &app_data_str)?;
    *state.0.lock().unwrap() = Some(abs_path);
    Ok(PlayerInfo { src, duration, subtitle })
```
（注意：`db` 参数在 `player_open` 签名里已存在为 `db: tauri::State<crate::db::Db>`，`resolve_subtitle` 接收 `&crate::db::Db`，传 `&db` 即可——`State` 解引用到内部值。若类型不匹配，用 `resolve_subtitle(db.inner(), ...)`。）

- [ ] **Step 6: 后端整体编译 + 测试通过**

Run: `cd /Users/zhoumo/Documents/Claude/collector/src-tauri && cargo test --lib 2>&1 | tail -25`
Expected: 编译成功，全部测试 `ok`（含新增 3 个与原有测试）。

- [ ] **Step 7: 提交**

```bash
cd /Users/zhoumo/Documents/Claude/collector
git add src-tauri/src/player/mod.rs
git commit -m "feat(player): player_open 返回字幕绝对路径(按 category_path+title 查库)

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>"
```

---

### Task 4: 前端 ipc 类型加 subtitle

**Files:**
- Modify: `src/lib/ipc.ts`（`playerOpen` 返回类型）

- [ ] **Step 1: 给 playerOpen 返回类型加 subtitle**

在 `src/lib/ipc.ts` 找到 `playerOpen`（现返回 `{ src: string; duration: number }`），改为：
```ts
  playerOpen: (category: string, categoryPath: string, title: string) =>
    invoke<{ src: string; duration: number; subtitle: string | null }>("player_open", { category, categoryPath, title }),
```

- [ ] **Step 2: 类型检查通过**

Run: `cd /Users/zhoumo/Documents/Claude/collector && npx tsc --noEmit`
Expected: 无错误。

- [ ] **Step 3: 提交**

```bash
cd /Users/zhoumo/Documents/Claude/collector
git add src/lib/ipc.ts
git commit -m "feat: ipc playerOpen 返回类型加 subtitle 字段

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>"
```

---

### Task 5: SubtitleRenderer 组件（封装 JASSUB）

**Files:**
- Create: `src/components/SubtitleRenderer.ts`

- [ ] **Step 1: 写实现**

Create `src/components/SubtitleRenderer.ts`:
```ts
import JASSUB from "jassub";
// Vite 资源导入：worker 与 wasm 由 Vite 打包并给出可访问 URL
import workerUrl from "jassub/dist/jassub-worker.js?worker&url";
import wasmUrl from "jassub/dist/jassub-worker.wasm?url";
import modernWasmUrl from "jassub/dist/jassub-worker-modern.wasm?url";

// 打包的 CJK 兜底字体（保证中文不方框，跨平台一致）
const cjkFontUrl = new URL("../assets/fonts/NotoSansCJKsc-Regular.woff2", import.meta.url).href;
const CJK_FONT_FAMILY = "Noto Sans CJK SC";

/**
 * 封装 JASSUB：在给定 <video> 上叠加 canvas 渲染 .ass 字幕，时间轴由 JASSUB 内部
 * 跟随 video.currentTime 同步（seek/暂停/倍速自动跟随）。
 * 初始化失败不抛出——仅 console.error，视频照常无字幕播放。
 */
export class SubtitleRenderer {
  private instance: JASSUB | null = null;

  constructor(video: HTMLVideoElement, subUrl: string) {
    try {
      this.instance = new JASSUB({
        video,
        subUrl,
        workerUrl,
        wasmUrl,
        modernWasmUrl,
        // 兜底字体：libass 找不到字幕指名字体时用它（中文不方框）
        availableFonts: { [CJK_FONT_FAMILY.toLowerCase()]: cjkFontUrl },
        defaultFont: CJK_FONT_FAMILY,
      });
    } catch (e) {
      console.error("[subtitle] JASSUB init failed", e);
      this.instance = null;
    }
  }

  /** 显示/隐藏字幕层（CC 开关）。 */
  setVisible(visible: boolean): void {
    if (!this.instance) return;
    try {
      // JASSUB 把 canvas 挂在 video 附近；用其 canvas 显隐控制。
      const canvas = (this.instance as unknown as { canvas?: HTMLCanvasElement }).canvas;
      if (canvas) canvas.style.display = visible ? "" : "none";
    } catch (e) {
      console.error("[subtitle] setVisible failed", e);
    }
  }

  /** 释放 worker/wasm/canvas（退出播放或换片必须调用）。 */
  destroy(): void {
    if (!this.instance) return;
    try {
      this.instance.destroy();
    } catch (e) {
      console.error("[subtitle] destroy failed", e);
    }
    this.instance = null;
  }
}
```

> 说明：JASSUB 完全类型化。`setVisible` 里访问 `canvas` 用了类型断言兜底——实现时若 `instance.canvas` 类型直接可用则去掉断言直接用；若库提供了更合适的显隐 API（如某个 `hide()`/样式属性），以 `node_modules/jassub` 的 `.d.ts` 为准替换，保持「显隐字幕层」语义不变。`destroy()` 是 JASSUB 文档化的释放方法。

- [ ] **Step 2: 确认 jassub 的 destroy / canvas API（查类型定义）**

Run:
```bash
cd /Users/zhoumo/Documents/Claude/collector
grep -nE "destroy|canvas|setTrackByUrl|freeTrack|ready" node_modules/jassub/dist/jassub.d.ts 2>/dev/null | head -30
```
Expected: 能看到 `destroy(): void`、`canvas` 属性等声明。据此确认/微调 Step 1 中方法名与属性名（若 `.d.ts` 路径不同，用 `find node_modules/jassub -name "*.d.ts"` 定位）。

- [ ] **Step 3: 类型检查通过**

Run: `cd /Users/zhoumo/Documents/Claude/collector && npx tsc --noEmit`
Expected: 无错误。若 jassub 无内置类型导致 `import JASSUB` 报错，检查 `node_modules/jassub` 是否含 `.d.ts` 并确认 `package.json` 的 `types` 字段；jassub 2.5.16 自带类型，正常应无需额外 `@types`。

- [ ] **Step 4: 提交**

```bash
cd /Users/zhoumo/Documents/Claude/collector
git add src/components/SubtitleRenderer.ts
git commit -m "feat: SubtitleRenderer 组件封装 JASSUB(含 CJK 兜底字体)

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>"
```

---

### Task 6: PlayerView 接入字幕 + CC 开关 + 构建验证

**Files:**
- Modify: `src/views/PlayerView.ts`（import、播放条 HTML、初始化、CC 按钮、cleanup）
- Modify: `src/styles/theme.css`（CC 按钮开启态样式）

- [ ] **Step 1: 加 import**

在 `src/views/PlayerView.ts` 顶部 import 区（`import { esc } ...` 下一行）新增：
```ts
import { convertFileSrc } from "@tauri-apps/api/core";
import { SubtitleRenderer } from "../components/SubtitleRenderer";
```

- [ ] **Step 2: 播放条 HTML 插入 CC 按钮**

在 `.player-bar` 里 `.vol` 与 `.fs` 之间插入 CC 按钮。把这段：
```ts
      <input class="vol" type="range" min="0" max="100" value="100"/>
      <button class="fs">${icon("fullscreen", 18)}</button>
```
改为：
```ts
      <input class="vol" type="range" min="0" max="100" value="100"/>
      <button class="cc" title="字幕" style="display:none">${icon("captions", 18)}</button>
      <button class="fs">${icon("fullscreen", 18)}</button>
```
（初始 `display:none`，仅本条有字幕时才显示。）

- [ ] **Step 3: 初始化字幕 + CC 交互**

在 `PlayerView` 函数体内，先在变量区（`let closed = false;` 附近）加：
```ts
  let sub: SubtitleRenderer | null = null;
  let subtitleOn = true;
  const ccBtn = el.querySelector<HTMLButtonElement>(".cc")!;
```

在 `try { ... const info = await api.playerOpen(...); ... video.src = info.src; video.load(); }` 块中，`video.src = info.src;` 之后、`video.load();` 之前（或紧随其后）加字幕初始化：
```ts
      if (info.subtitle) {
        sub = new SubtitleRenderer(video, convertFileSrc(info.subtitle));
        ccBtn.style.display = "";
        ccBtn.classList.add("cc-on");
      }
```

在其它按钮绑定附近，加 CC 按钮点击：
```ts
  ccBtn.onclick = () => {
    subtitleOn = !subtitleOn;
    sub?.setVisible(subtitleOn);
    ccBtn.classList.toggle("cc-on", subtitleOn);
  };
```

- [ ] **Step 4: cleanup 释放字幕**

在 `cleanup` 函数体里（`video.pause();` 之后）加：
```ts
    sub?.destroy();
    sub = null;
```

- [ ] **Step 5: CC 按钮开启态样式**

在 `src/styles/theme.css` 的播放器区（`.player-bar button{...}` 之后）追加：
```css
.player-bar .cc.cc-on{color:var(--accent)}
.player-bar .cc.cc-on .icon{filter:drop-shadow(0 0 4px var(--accent-glow))}
```
（开启时字幕图标用强调色 + 轻微辉光，关闭时恢复默认色。）

- [ ] **Step 6: 类型检查通过**

Run: `cd /Users/zhoumo/Documents/Claude/collector && npx tsc --noEmit`
Expected: 无错误。

- [ ] **Step 7: 完整构建通过（确认 jassub worker/wasm/字体被打包）**

Run: `cd /Users/zhoumo/Documents/Claude/collector && npm run build 2>&1 | tail -30`
Expected: `tsc` 与 `vite build` 均成功。产物 `dist/assets/` 下应出现 jassub 的 worker js、`.wasm` 文件与字体 woff2（Vite 处理 `?url`/`import.meta.url` 资源）。用以下确认：
```bash
ls dist/assets/ | grep -iE "jassub|worker|\.wasm|woff2|NotoSans" || echo "警告：未见 jassub/字体产物"
```
Expected: 列出 worker、wasm、woff2 相关文件。若缺失，检查 Task 5 的资源导入写法。

- [ ] **Step 8: 提交**

```bash
cd /Users/zhoumo/Documents/Claude/collector
git add src/views/PlayerView.ts src/styles/theme.css
git commit -m "feat(player): 接入字幕渲染 + CC 开关(音量与全屏之间)

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>"
```

---

### Task 7: 真机验证

**Files:** 无（手动验收）

- [ ] **Step 1: 启动应用**

```bash
cd /Users/zhoumo/Documents/Claude/collector
lsof -ti:1420 | xargs kill -9 2>/dev/null; pkill -f "tauri dev" 2>/dev/null; sleep 1
nohup npm run tauri dev > /tmp/collector-dev.log 2>&1 & disown
```
等待窗口起来（`tail -f /tmp/collector-dev.log`）。

- [ ] **Step 2: 逐项验收（在应用里操作）**

- 播放一条有字幕的视频 → 字幕出现在画面上，中文正常不方框，双语两行，样式（颜色/描边/定位）与 PotPlayer/VLC 观感一致。
- 拖动进度条 seek、快进/快退 → 字幕时间轴精确跟随。
- 点 CC 按钮 → 字幕隐藏、按钮恢复默认色；再点 → 字幕恢复、按钮高亮。
- 返回列表再进另一条视频 → 无上一条字幕残留；多次进出仍正常（无 worker 泄漏、无报错）。
- 看 `/tmp/collector-dev.log` 与浏览器控制台无 `[subtitle]` 报错。
- （可选，验「无字幕」分支）临时把某条 `subtitle_path` 置空测：
  `sqlite3 "$HOME/Library/Application Support/com.zhoumo.collector/collector.sqlite" "UPDATE media SET subtitle_path='' WHERE id=(SELECT id FROM media LIMIT 1);"` → 播放该条 → CC 按钮不显示、视频正常播。**验完记得改回**（重新扫描或手动恢复原值）。

- [ ] **Step 3: 关闭应用**

```bash
lsof -ti:1420 | xargs kill -9 2>/dev/null; pkill -f "tauri dev" 2>/dev/null
```

---

## 自审（对照 spec）

- **spec 覆盖**：完整还原 .ass → JASSUB（Task 1+5）；player_open 返回字幕 → Task 3；convertFileSrc 交 subUrl → Task 6 Step 3；CJK 兜底字体跨平台 → Task 1+5；CC 开关在音量/全屏之间 + captions 图标 → Task 2+6；自动显示 → Task 6 Step 3；无字幕不显示按钮 → Task 6（`display:none` + 仅有字幕才显示）；destroy 释放 → Task 5+6 Step 4；时间轴同步 → JASSUB 内部（Task 5 依赖 video 引用）。全部有对应任务。
- **占位符**：无 TBD/TODO，每个代码步骤含完整代码；字体下载与库 API 名两处标了「以实际为准」的验证步骤（Step 有明确 Expected 与兜底措施），非占位。
- **类型一致**：后端 `PlayerInfo.subtitle: Option<String>`（Task 3）↔ 前端 `subtitle: string | null`（Task 4）一致；`SubtitleRenderer(video, subUrl)` / `setVisible(bool)` / `destroy()`（Task 5 定义，Task 6 消费）一致；`icon("captions")`（Task 2 定义，Task 6 用）一致；`resolve_subtitle(db, category_path, title, app_data)` 测试与实现签名一致。

## 风险

- **jassub 的 canvas/destroy API 名**：Task 5 Step 2 用 grep `.d.ts` 兜底确认，以类型定义为准微调，语义不变。
- **字体下载源**：Task 1 Step 3/4 有 `file` 校验 + 换源兜底（fontsource CDN / npm 包），防下到错误页。
- **Vite 打包 jassub 资源**：Task 6 Step 7 显式检查 dist 产物，缺失则回查导入写法；CSP null、单线程 fallback 使即便无 SharedArrayBuffer 也能跑。
- **全部条目当前都有字幕**：真机「无字幕」分支需临时改库验证（Task 7 Step 2 给了可回滚的方法）。
