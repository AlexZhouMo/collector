# 编辑封面：悬浮删除

## 目标

编辑视频抽屉的封面预览区：有封面时鼠标悬浮显示删除图标，点击后本地清空预览；点「保存」时真正清空数据库 cover_path 并删除原磁盘图片。点「取消」不删（可反悔）。

## 决策汇总

| 项 | 决策 |
|----|------|
| 删除生效时机 | 保存时生效（点删除只清预览，保存才删库+删文件，取消可反悔） |
| 共用保护 | 不检查，直接删文件（手动封面多为独立，简单直接） |
| 删除图标 | 封面右上角，容器 hover 时显示 |

## 改动方案

### 前端 EditDrawer.ts
- 封面预览包容器 `.cover-box`（相对定位），内含 `<img data-d="cover">` + 删除按钮 `.cover-del`（绝对定位右上角，默认隐藏，容器 hover 显示）。
- 记录 `const originalCover = item?.cover_path ?? ""`（保存时判断是否删旧文件）。
- 删除按钮 onclick：`coverPath = ""; refreshCover();`（预览与删除图标随空封面消失）。
- `refreshCover`：有 coverPath 显示 img + 容器；无则隐藏整个 `.cover-box`。
- 保存时：先判断——若 `originalCover` 非空 且 `originalCover !== coverPath`（被删或被换），调 `api.deleteCoverFile(originalCover)` 删旧文件（失败忽略，不阻断保存）；再 `api.mediaUpdate(... coverPath || null ...)`。

### 后端 lib.rs 新增命令
```rust
/// 删除 covers 目录内的封面文件。仅允许删 <app_data>/covers/ 下的文件（防路径穿越）。
#[tauri::command(rename_all = "camelCase")]
fn delete_cover_file(app: tauri::AppHandle, path: String) -> AppResult<()> {
    let covers = app.path().app_data_dir()?.join("covers");
    let target = std::path::Path::new(&path);
    // 安全校验：target 必须在 covers 目录内
    let canon_covers = covers.canonicalize().unwrap_or(covers.clone());
    match target.canonicalize() {
        Ok(ct) if ct.starts_with(&canon_covers) => {
            std::fs::remove_file(&ct).ok(); // 不存在忽略
            Ok(())
        }
        _ => Ok(()), // 目录外或不存在，静默忽略（不误删）
    }
}
```
注册到 generate_handler!。

### 前端 ipc.ts
`deleteCoverFile: (path: string) => invoke<void>("delete_cover_file", { path })`。

### CSS（theme.css）
- `.cover-box{position:relative;display:inline-block}`
- `.cover-del`：绝对定位右上角、圆形半透明底、红色删除图标、默认 `opacity:0`，`.cover-box:hover .cover-del{opacity:1}`。

## 测试策略

- 后端 `delete_cover_file`：单测——covers 目录内文件能删；目录外路径拒绝（不删）；不存在文件不报错。
- 前端手动验证：编辑有封面的视频 → 悬浮出现删除图标 → 点击预览消失 → 保存后库中 cover_path 空、磁盘文件删除；点取消则不变。

## 明确不做（YAGNI）

- 不做删除前共用引用检查（直接删）。
- 不做立即删除（统一保存时生效）。
- 不做删除确认弹窗（点删除只是清预览，未真删，保存前可反悔）。

## 风险

- 若封面文件被同季其他条目共用（TMDB 抓取去重），删除会连带影响它们的显示——已与用户确认接受（手动封面多为独立）。
- delete_cover_file 用 canonicalize 校验路径在 covers 内，防止传入任意路径误删系统文件。
