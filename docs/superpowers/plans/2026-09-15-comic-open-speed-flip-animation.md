# 漫画打开加速 + 翻页动画放慢 实现计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 让漫画卷打开首屏秒级可读，翻页 3D 卷动动画放慢到 1.3s 有明显重量感。

**Architecture:** 后端页尺寸读取从「整卷全解压」改为「流式只读图片头部」（A）；前端打开分两阶段——先按竖单页秒开首页、后端头部尺寸算完经 Tauri 事件回传后就地重排对开（B）；前端翻页时预取下一对开图片使翻页跟手（C）；CSS 翻页动画时长 0.72s→1.3s。

**Tech Stack:** Rust（zip 2 / image 0.25 / tauri spawn_blocking + emit）、TypeScript（vanilla-ts）、Tauri 2 事件（`@tauri-apps/api/event` 的 `listen`）、CSS transition。

---

## 文件结构

- `src-tauri/src/comic/reader.rs` — 改 `list_pages_with_dims` 为流式只读头部；新增按需读头部的辅助函数与测试。
- `src-tauri/src/comic/mod.rs` — 保持 `comic_pages`；新增 `comic_page_names`（只列页名，快速）与后台补正无需新命令（尺寸仍走 `comic_pages`，前端在阶段二调用）。
- `src/lib/ipc.ts` — 新增 `comicPageNames` 绑定。
- `src/views/ComicReaderView.ts` — 打开两阶段化、尺寸补正重排、下一对开预取。
- `src/styles/theme.css` — 翻页动画时长放慢。

> 说明：B 方案的「后台尺寸」直接复用现有 `comic_pages`（已在 `spawn_blocking` 中、已被方案 A 提速），前端在阶段一渲染后再 await 它，拿到尺寸即重排。无需新增事件通道，比 emit 更简单且等价（前端 await 本身即异步不阻塞首屏）。这样收敛了 spec 中「后台补正」的实现方式为「阶段二 await」，避免多引入一条事件链路。

---

## Task 1: 后端页尺寸读取改为流式只读头部（加速 A）

**Files:**
- Modify: `src-tauri/src/comic/reader.rs`（`list_pages_with_dims` 函数体，约 46-71 行）
- Test: `src-tauri/src/comic/reader.rs`（`#[cfg(test)] mod tests`）

- [ ] **Step 1: 新增测试——验证只读头部路径对正常图片返回正确尺寸**

在 `reader.rs` 的 `mod tests` 内新增测试（放在现有 `list_pages_with_dims_reads_size` 之后）：

```rust
    #[test]
    fn list_pages_with_dims_reads_size_without_full_decode() {
        use image::{RgbImage, Rgb};
        let tmp = tempfile::tempdir().unwrap();
        let zp = tmp.path().join("c.zip");
        let f = File::create(&zp).unwrap();
        let mut w = zip::ZipWriter::new(f);
        let opt = SimpleFileOptions::default();
        // 造一张较大的图（尺寸信息在头部，整图字节较多），验证仍能拿到尺寸
        let mut buf = std::io::Cursor::new(Vec::new());
        RgbImage::from_pixel(1000, 1400, Rgb([9, 9, 9]))
            .write_to(&mut buf, image::ImageFormat::Jpeg)
            .unwrap();
        w.start_file("001.jpg", opt).unwrap();
        w.write_all(buf.get_ref()).unwrap();
        w.finish().unwrap();
        let pages = list_pages_with_dims(&zp).unwrap();
        assert_eq!(pages.len(), 1);
        assert_eq!((pages[0].w, pages[0].h), (1000, 1400));
    }
```

- [ ] **Step 2: 运行测试，确认现有实现下通过（作为行为基线）**

Run: `cd src-tauri && cargo test --lib comic::reader 2>&1 | tail -20`
Expected: 全部 PASS（新测试在旧实现下也应通过，因为旧实现虽全解压但尺寸正确——本测试锁定行为，后续改实现不得回归）。

- [ ] **Step 3: 改 `list_pages_with_dims` 为流式只读头部**

将 `list_pages_with_dims` 中读取每页字节的部分替换为「渐进读入 + 尝试解析尺寸」。完整函数改为：

```rust
/// 列页并取每页宽高。只读图片头部尺寸（不解码整张像素，也不把整页字节读入内存）：
/// 从 zip 条目流式读入小块，累积到能解析出尺寸即停。整卷可能上百页、数百 MB，
/// 全解压会阻塞数十秒；流式读头部把开销降到毫秒级。
/// 尺寸解析失败按 (0,0)（前端按竖单页默认处理）。
pub fn list_pages_with_dims(zip_path: &Path) -> AppResult<Vec<PageInfo>> {
    let names = list_pages(zip_path)?;
    let file = File::open(zip_path)?;
    let mut ar = ZipArchive::new(file).map_err(|e| AppError::Other(e.to_string()))?;
    let mut out = Vec::with_capacity(names.len());
    for name in names {
        let (w, h) = match ar.by_name(&name) {
            Ok(f) => read_dims_from_head(f).unwrap_or((0, 0)),
            Err(_) => (0, 0),
        };
        out.push(PageInfo { name, w, h });
    }
    Ok(out)
}

/// 从一个可读流（zip 条目）渐进读入头部字节，尝试解析图片尺寸。
/// 分块累积（每块 64KB），每次累积后尝试解析；解析成功即返回，不再继续读。
/// 头部特别大或渐进式编码时会多读几块，最坏读完整条目——但仍不解码像素。
fn read_dims_from_head<R: Read>(mut r: R) -> Option<(u32, u32)> {
    let mut buf: Vec<u8> = Vec::with_capacity(64 * 1024);
    let mut chunk = [0u8; 64 * 1024];
    loop {
        let n = r.read(&mut chunk).ok()?;
        if n == 0 {
            break; // 读完仍未解析出，返回 None
        }
        buf.extend_from_slice(&chunk[..n]);
        if let Ok(reader) = image::ImageReader::new(std::io::Cursor::new(&buf)).with_guessed_format() {
            if let Ok(dims) = reader.into_dimensions() {
                return Some(dims);
            }
        }
        // 上限保护：累计超过 8MB 仍未解析成功，放弃（几乎不可能是正常头部）
        if buf.len() > 8 * 1024 * 1024 {
            break;
        }
    }
    None
}
```

- [ ] **Step 4: 运行全部 reader 测试，确认通过**

Run: `cd src-tauri && cargo test --lib comic::reader 2>&1 | tail -20`
Expected: 全部 PASS，含 `lists_sorted_image_pages_only`、`list_pages_with_dims_reads_size`、`list_pages_with_dims_corrupt_image_degrades_to_zero`、`reads_entry_bytes`、新增的 `..._without_full_decode`。

- [ ] **Step 5: 提交**

```bash
git add src-tauri/src/comic/reader.rs
git commit -m "perf(comic): 页尺寸改为流式只读图片头部，不再整卷全解压"
```

---

## Task 2: 新增只列页名的快速命令（加速 B 阶段一）

**Files:**
- Modify: `src-tauri/src/comic/mod.rs`（新增命令）
- Modify: `src-tauri/src/lib.rs`（注册命令到 invoke_handler）
- Modify: `src/lib/ipc.ts`（新增绑定）

- [ ] **Step 1: 在 `comic/mod.rs` 新增 `comic_page_names` 命令**

在 `comic_pages` 命令下方新增（复用 `reader::list_pages`，只读中央目录、毫秒级，不读任何图片字节）：

```rust
/// 只列漫画卷各页名（不含尺寸），毫秒级。用于打开卷时先按竖单页秒开首页，
/// 尺寸随后由 comic_pages 异步补齐重排。放 spawn_blocking 与 comic_pages 一致。
#[tauri::command]
pub async fn comic_page_names(path: String) -> AppResult<Vec<String>> {
    tauri::async_runtime::spawn_blocking(move || reader::list_pages(Path::new(&path)))
        .await
        .map_err(|e| crate::error::AppError::Other(format!("join: {e}")))?
}
```

- [ ] **Step 2: 在 `lib.rs` 注册该命令**

找到 `tauri::generate_handler![` 列表中已有的 `comic_pages`，在其后添加 `comic_page_names`。先定位：

Run: `grep -n "comic_pages" src-tauri/src/lib.rs`

在 `comic::comic_pages,`（或 `comic_pages,`，按现有写法）同样风格后追加一行 `comic::comic_page_names,`（保持与现有条目相同的路径前缀写法——先看该行确认前缀）。

- [ ] **Step 3: 在 `ipc.ts` 新增绑定**

在 `comicPages` 那行下方新增：

```typescript
  comicPageNames: (path: string) => invoke<string[]>("comic_page_names", { path }),
```

- [ ] **Step 4: 编译验证**

Run: `cd src-tauri && cargo build 2>&1 | tail -20`
Expected: 编译通过，无 unused/未注册命令错误。

- [ ] **Step 5: 提交**

```bash
git add src-tauri/src/comic/mod.rs src-tauri/src/lib.rs src/lib/ipc.ts
git commit -m "feat(comic): 新增 comic_page_names 快速列页命令（供首屏秒开）"
```

---

## Task 3: 阅读器打开两阶段化 + 尺寸补正重排（加速 B）

**Files:**
- Modify: `src/views/ComicReaderView.ts`

- [ ] **Step 1: 改打开流程——阶段一按页名秒开，阶段二 await 尺寸重排**

将文件顶部读取与初始化部分（当前 `const pages = await api.comicPages(...)` 到 `let idx = 0;` 一段）替换为两阶段逻辑。

把原来的：

```typescript
  const pages = await api.comicPages(vol.zip_path);
  const spreads = buildSpreads(pages);
  let idx = 0;
```

替换为：

```typescript
  // 阶段一：只取页名，全部按竖单页秒开（w/h=0 → buildSpreads 视作竖页）
  const names = await api.comicPageNames(vol.zip_path);
  let pages = names.map((name) => ({ name, w: 0, h: 0 }));
  let spreads = buildSpreads(pages);
  let idx = 0;
```

- [ ] **Step 2: 在首屏 render 之后、返回 el 之前，加入阶段二尺寸补正**

文件末尾当前为：

```typescript
  await render();
  return el;
}
```

替换为：

```typescript
  await render();

  // 阶段二：后台取带尺寸的页表，重算对开分组并就地校正。
  // 用「当前显示的页名」重新定位 idx，避免重排后页码跳动；
  // 若正在翻页动画中，推迟到动画结束再应用。
  (async () => {
    let dimsPages;
    try {
      dimsPages = await api.comicPages(vol.zip_path);
    } catch {
      return; // 尺寸取失败：保持竖单页分组，不影响阅读
    }
    if (closed) return;
    const applyDims = () => {
      if (closed) return;
      // 记录当前对开首个页名，用于重排后重定位
      const anchorName = (spreads[idx]?.left ?? spreads[idx]?.right)?.name;
      pages = dimsPages;
      spreads = buildSpreads(pages);
      // 重定位 idx 到含 anchorName 的对开
      if (anchorName) {
        const ni = spreads.findIndex(
          (sp) => sp.left?.name === anchorName || sp.right?.name === anchorName,
        );
        if (ni >= 0) idx = ni;
      }
      if (idx >= spreads.length) idx = Math.max(0, spreads.length - 1);
      render();
    };
    if (flipping) {
      const iv = window.setInterval(() => {
        if (closed) { clearInterval(iv); return; }
        if (!flipping) { clearInterval(iv); applyDims(); }
      }, 100);
    } else {
      applyDims();
    }
  })();

  return el;
}
```

- [ ] **Step 3: 新增 `closed` 标志并在 cleanup 中置位**

当前 `(el as any)._cleanup` 中未有 `closed`。在 `let flipping = false;` 附近（`go` 定义之前）新增：

```typescript
  let closed = false;
```

并把 `(el as any)._cleanup` 改为在开头置位 `closed`：

```typescript
  (el as any)._cleanup = () => {
    closed = true;
    window.removeEventListener("keydown", onKey);
    window.removeEventListener("mousemove", onMove);
    window.removeEventListener("mouseup", onUp);
  };
```

- [ ] **Step 4: 类型检查**

Run: `npx tsc --noEmit 2>&1 | tail -20`
Expected: 无类型错误（`pages`/`spreads` 由 `const` 改 `let` 后仍类型一致；`buildSpreads` 入参为 `PageInfo[]`，`{name,w:0,h:0}` 满足）。

- [ ] **Step 5: 提交**

```bash
git add src/views/ComicReaderView.ts
git commit -m "feat(comic): 阅读器首屏按页名秒开，尺寸后台补正重排"
```

---

## Task 4: 翻页时预取下一对开图片（加速 C）

**Files:**
- Modify: `src/views/ComicReaderView.ts`（`render` 函数末尾）

- [ ] **Step 1: 在 `render` 完成后预取相邻对开**

当前 `render` 末尾为：

```typescript
    pager.textContent = `${idx + 1} / ${spreads.length}`;
    updateNav();
  };
```

替换为（新增预取：加载下一对开与上一对开的图片进 cache，不 await 不阻塞当前渲染；`load` 已有 cache 去重）：

```typescript
    pager.textContent = `${idx + 1} / ${spreads.length}`;
    updateNav();
    // 预取相邻对开图片（存入 cache），使翻页动画启动即有内容、不等 IO。
    // 不 await：后台预热；load 内部有 cache，重复调用无副作用。
    const prefetch = (i: number) => {
      const sp = spreads[i];
      if (!sp) return;
      if (sp.left) void load(sp.left.name);
      if (sp.right) void load(sp.right.name);
    };
    prefetch(idx + 1);
    prefetch(idx - 1);
  };
```

- [ ] **Step 2: 类型检查**

Run: `npx tsc --noEmit 2>&1 | tail -20`
Expected: 无类型错误。

- [ ] **Step 3: 提交**

```bash
git add src/views/ComicReaderView.ts
git commit -m "perf(comic): 翻页预取相邻对开图片，翻页更跟手"
```

---

## Task 5: 翻页动画放慢到 1.3s

**Files:**
- Modify: `src/styles/theme.css`（约 300-317 行 flipper/shade/glare）
- Modify: `src/views/ComicReaderView.ts`（`go` 内兜底 setTimeout）

- [ ] **Step 1: 改 flipper transition 时长**

在 `theme.css` 找到（约 301 行）：

```css
.book .flipper{position:absolute;top:0;bottom:0;left:50%;right:0;transform:rotateY(0deg);
```

该规则块内包含 `transition:transform .72s cubic-bezier(.36,.05,.2,1)`（承接第 300 行注释「约 0.72s」）。将其 `.72s` 改为 `1.3s`。若 transition 值分布在该多行规则中，定位到含 `transition:transform` 的那行，把 `.72s` 改为 `1.3s`。同时把第 300 行注释 `约 0.72s` 改为 `约 1.3s`。

- [ ] **Step 2: 改 shade / glare 动画时长**

找到（约 312、317 行）：

```css
.flipper.flip-next .shade{animation:flipShade .72s cubic-bezier(.36,.05,.2,1) forwards}
```
```css
.flipper.flip-next .glare{animation:flipGlare .72s cubic-bezier(.36,.05,.2,1) forwards}
```

两处 `.72s` 均改为 `1.3s`。

- [ ] **Step 3: 改 `go` 内兜底 setTimeout**

在 `ComicReaderView.ts` 的 `go` 函数中找到：

```typescript
    setTimeout(() => { if (flipping) done(); }, 900);
```

改为（须大于动画时长 1.3s）：

```typescript
    setTimeout(() => { if (flipping) done(); }, 1500);
```

- [ ] **Step 4: 类型检查 + 确认无遗漏 0.72s**

Run: `npx tsc --noEmit 2>&1 | tail -5 && grep -n "\.72s\|,900)" src/styles/theme.css src/views/ComicReaderView.ts`
Expected: tsc 无错误；grep 无 `.72s`（flipper 相关）残留、无旧的 `900` 兜底。

- [ ] **Step 5: 提交**

```bash
git add src/styles/theme.css src/views/ComicReaderView.ts
git commit -m "style(comic): 翻页卷动动画放慢至 1.3s，增强重量感"
```

---

## Task 6: 整体手动验证（全流程）

**Files:** 无（验证任务，不改代码）

- [ ] **Step 1: 构建前端 + 后端确认无误**

Run: `npx tsc --noEmit && cd src-tauri && cargo build 2>&1 | tail -10`
Expected: 均通过。

- [ ] **Step 2: 记录手动验证清单（由控制者在真实应用中确认）**

打开一个较大的漫画卷，逐项确认：
- 打开后首页（封面）几乎立即出现，不再等待整卷（B + A 生效）。
- 尺寸补正后，横向对开页正确合并为一屏、竖页两两对开（阶段二 `buildSpreads` 生效），且补正瞬间当前所见页不跳。
- 连续点「下一页」/方向键翻页，翻页动画启动即有图（C 预取生效），无「点了等一下才翻」。
- 翻页卷动动画明显放慢（约 1.3s），能看清书页转动；连续快翻时 `flipping` 锁生效、无残留 flipper。
- 缩放/平移、封面单页居中、末页落单等既有行为不变。

- [ ] **Step 3: 若全部通过，无需额外提交（前序任务已各自提交）**

---

## 自审记录

- **Spec 覆盖**：A→Task 1；B→Task 2（快速命令）+Task 3（两阶段+重排）；C→Task 4（前端预取；spec 已声明后端 zip 缓存默认不做）；翻页 1.3s→Task 5；验证→Task 6。全部覆盖。
- **实现收敛**：spec 中 B 的「后台 emit 补正」在计划里收敛为「阶段二 await comic_pages」——等价、更简单、不新增事件链路；已在文件结构说明中标注。
- **类型一致**：`comicPageNames` 返回 `string[]`；`pages`/`spreads` 由 `const`→`let`；`closed`/`flipping` 均在 `go` 前声明；`load`/`render`/`buildSpreads` 签名不变。
- **无占位符**：所有代码步骤含完整代码与确切命令。
