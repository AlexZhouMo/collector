# 编辑封面：上传图片 + 2:3 裁剪 + 生成标准海报

## 目标

编辑视频抽屉中点「选择图片」，选一张主流格式图片（jpg/png/webp）后弹出裁剪弹窗，用户以 2:3 比例锁定的裁剪框选定区域，后端按裁剪区域生成 **500×750 JPEG q85** 海报——格式与 TMDB 自动抓取的海报完全一致（复用同一段缩放+编码代码）。

## 现状与问题

当前编辑抽屉「选择图片」流程：选文件 → 后端 `import_cover` **原样拷贝**到 `<app_data>/covers/`，不裁剪、不规范尺寸。因此手动封面与 TMDB 抓取的 500×750 规格不一致（`poster/image_proc.rs::to_cover` 才做规范化）。本设计让手动封面也走标准化，并加入用户可控的裁剪区域选择。

## 决策汇总

| 项 | 决策 |
|----|------|
| 裁剪界面 | 自建裁剪弹窗（DOM/绝对定位，无新依赖） |
| 裁剪交互 | 拖动裁剪框位置 + 拖角/边缩放，比例锁定 2:3 |
| 海报生成 | 后端生成：前端只传原图路径 + 裁剪矩形(x,y,w,h)，后端复用 to_cover 的缩放+编码 |
| 格式一致性 | crop_to_cover 与 to_cover 共用 finalize（500×750 + JPEG q85），必然一致 |
| 支持格式 | jpg/jpeg/png/webp（image crate 解码支持的主流格式） |

## 架构与数据流

```
编辑抽屉「选择图片」
  → 文件选择器选图（jpg/png/webp）
  → openCoverCropper(srcPath, onDone)：弹裁剪窗，显示原图 + 2:3 裁剪框
     用户拖动/缩放框（比例锁 2:3）
     「确定」→ 屏幕坐标换算为原图像素坐标 (x,y,w,h)
       → api.importCoverCropped(srcPath, x, y, w, h)
         后端：读原图字节 → crop_to_cover(bytes,x,y,w,h) → save_cover → 返回封面绝对路径
       → onDone(coverPath)：回填 coverPath + 刷新预览
     「取消」→ 关闭不改
```

## 模块拆分

### 前端 `src/components/CoverCropper.ts`（新文件，单一职责）
- 导出 `openCoverCropper(srcPath: string, onDone: (coverPath: string) => void): void`。
- 用 `convertFileSrc(srcPath)` 显示原图（`<img>`）。
- 裁剪框：绝对定位的 div 覆盖在图上，四角 handle 缩放、框体拖动，比例始终锁 2:3。
- **坐标换算（关键）**：图片 `<img>` 加载后取 `naturalWidth/naturalHeight`（原图像素）与 `clientWidth/clientHeight`（显示尺寸），算缩放比 `scale = natural/client`；裁剪框相对图片左上的屏幕坐标 × scale = 原图像素坐标 (x,y,w,h)，并 clamp 到 [0, natural] 范围避免越界。
- 「确定」调 `api.importCoverCropped(srcPath, x, y, w, h)`，成功回调 onDone；失败弹提示不关闭。
- 弹窗样式复用现有 glass/drawer-overlay 风格；Esc/点遮罩/取消关闭。

### 前端 `src/components/EditDrawer.ts`（改动）
- 「选择图片」onclick：从「选文件 → importCover」改为「选文件 → openCoverCropper(f, (p)=>{ coverPath=p; refreshCover(); })」。

### 前端 `src/lib/ipc.ts`（改动）
- 加 `importCoverCropped: (srcImage, x, y, w, h) => invoke<string>("import_cover_cropped", { srcImage, x, y, w, h })`。

### 后端 `src-tauri/src/poster/image_proc.rs`（改动）
- 把 `to_cover` 的「缩放到 500×750 + JPEG q85 编码」抽为内部函数 `finalize(img: DynamicImage) -> AppResult<Vec<u8>>`，`to_cover` 改为调用它（行为不变，现有测试仍过）。
- 新增 `crop_to_cover(bytes: &[u8], x: u32, y: u32, w: u32, h: u32) -> AppResult<Vec<u8>>`：解码 → `crop_imm(x,y,w.max(1),h.max(1))` → `finalize`。与 to_cover 共用 finalize，格式必然一致。

### 后端 `src-tauri/src/lib.rs`（改动）
- 新命令：
```rust
#[tauri::command(rename_all = "camelCase")]
fn import_cover_cropped(app, src_image: String, x: u32, y: u32, w: u32, h: u32) -> AppResult<String> {
    let covers = app.path().app_data_dir()?.join("covers");
    let bytes = std::fs::read(&src_image)?; // 错误映射为 AppError
    let cover = poster::image_proc::crop_to_cover(&bytes, x, y, w, h)?;
    poster::image_proc::save_cover(&covers, &cover)
}
```
- 注册到 `generate_handler!`。保留旧 `import_cover` 命令（不删，向后兼容；编辑抽屉改用新命令）。

## 错误处理

- 非图片/损坏图 → `crop_to_cover` 解码失败返回 AppError → 前端弹窗提示「图片无法识别」，弹窗不关闭。
- 裁剪框越界 → 前端 clamp 到图片像素范围，不传非法坐标；后端 `crop_imm` 亦对超界安全（image crate 会截断到边界）。
- 读原图失败（文件不存在）→ 后端返回错误，前端提示。

## 测试策略

### 后端 `image_proc.rs`
- `crop_to_cover` 输出规格：构造 1000×1000 测试图，裁 `(100,100,600,900)` → 断言解码后尺寸恰为 500×750、JPEG 魔数 FF D8。
- 一致性：对同一张纯色图，`to_cover` 与 `crop_to_cover`（裁全图 0,0,w,h）输出应能各自解码为 500×750 JPEG（验证共用 finalize，不做字节级相等断言，因裁剪区域不同）。
- 现有 `to_cover` 测试保持通过（重构 finalize 后行为不变）。

### 前端
- 坐标换算逻辑可抽为纯函数 `screenRectToImageRect(rect, scale, natural)` 便于单测；裁剪框拖拽交互手工验证。

### 真机验证
1. 编辑某视频 → 选择图片 → 弹裁剪窗，显示原图 + 2:3 框。
2. 拖动/缩放框 → 确定 → 预览显示生成的海报，尺寸 2:3 不变形。
3. 保存后面板显示该封面，与 TMDB 抓取的海报尺寸/风格一致。
4. 选损坏文件 → 提示错误，不崩溃。

## 明确不做（YAGNI）

- 不支持旋转/滤镜（仅裁剪定位）。
- 不引入 cropperjs 等第三方库（自建足够）。
- 不改 TMDB 抓取流程（本功能仅手动封面）。
- 不删旧 import_cover 命令（保留兼容；编辑抽屉切到新命令）。

## 风险

- 前端屏幕坐标↔原图像素坐标换算若算错，会裁到错误区域——抽纯函数单测 + 真机核对。
- 超大图（如 8000px）显示时 `<img>` 会被 CSS 缩小，换算依赖 naturalWidth 正确；主流图片无问题。
- webp 解码依赖 image crate 的 webp feature——image 0.25 默认支持解码 webp，若不支持则该格式报错进错误提示（jpg/png 必支持）。
