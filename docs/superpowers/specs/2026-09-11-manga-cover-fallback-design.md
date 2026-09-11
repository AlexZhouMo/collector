# 漫画封面：AniList 停用识别(A) + 编辑抽屉手动上传(C) 设计文档

**日期**：2026-09-11
**状态**：已确认

## 背景与动机

漫画封面拓取报 `网络错误: anilist search: ... status code 403`。经排查根因是 **AniList 官方 API 临时停用**（响应体明确 "The AniList API has been temporarily disabled due to severe stability issues."，任何请求/UA 都返回同样的官方 403，非本项目代码/网络问题）。

两项应对（已确认 A+C）：
- **A**：识别 AniList 停用/不可用，给准确提示（不再笼统"网络错误"）。等 AniList 恢复即自动可用。
- **C**：给漫画上完整编辑抽屉（EditDrawer），支持手动上传/裁剪封面（不依赖任何网络源），复用视频的上传裁剪机制。

## A：AniList 停用识别

`fetch_manga_covers` 现把 search_cover 错误笼统包成 `网络错误: {e}`。改：
- `anilist::search_cover`：收到 403 时读响应体，若含 "temporarily disabled"（或状态 403），返回明确错误 `AppError::Other("AniList 服务暂时不可用")`（而非泛化网络错误）。
- `fetch_manga_covers`：该错误对应的 FailedItem.reason 显示"AniList 服务暂时不可用，请稍后重试或手动上传封面"。其它真实网络错误仍报"网络错误: ..."。
- 前端 NormalizeView 未命中列表原样展示 reason（不变）。

## C：漫画编辑抽屉（含手动封面）

### EditDrawer 参数化 kind
`openEditDrawer(item, onSaved, defaultCategory)` 加 `kind: "video" | "comic"` 参数（默认 "video" 保持视频调用不变）。漫画时：
- 文案"编辑漫画"（当前"编辑视频"）。
- 隐藏"视频路径"字段（漫画无视频路径）。
- 封面上传/裁剪复用现有 `openCoverCropper` + import_cover/import_cover_cropped（不变逻辑）。
- 分类/分类路径不在表单编辑（编辑保留原值，与视频一致）——漫画 category_path 是完整层级路径，原样保留。
- 保存调参数化后的 media 命令写 comic 表。
- 只编辑现有漫画，**不做"新增漫画"**（漫画由扫描/导入生成）。

### 后端 media 命令参数化表名
`media_update` / `media_create` / `media_delete` 加 `kind` 参数（或表名），内部 table = media/comic（内部常量、无注入）。漫画编辑走 comic 表。参考已有的 update_cover_path 参数化模式。
（media_create 漫画侧本次不接（C 不做新增漫画），但 update/delete 需支持 comic。）

### import_cover / import_cover_cropped 按 kind 存子目录
这两个命令现硬编码 `covers/media`。加 `kind` 参数：video→covers/media、comic→covers/comic。漫画编辑上传封面存 covers/comic/。

### 入口
ComicView 的 FolderView 现 onContext 传 undefined。改为传漫画右键菜单（编辑 → openEditDrawer(it, refresh, kind="comic"); 删除 → mediaDelete(comic)）。EditDrawer 保存后 refresh 重渲染。

## 组件改动
- 后端：anilist.rs search_cover（403→明确错误）；lib.rs media_update/media_delete 参数化 kind/表名 + import_cover/import_cover_cropped 加 kind 参数选 covers 子目录。
- 前端：EditDrawer 加 kind 参数（漫画文案/隐藏视频路径字段/写 comic）；ComicView 传漫画右键菜单（编辑/删除）；ipc：mediaUpdate/mediaDelete/importCover/importCoverCropped 加 kind 参数。

## 错误处理
- A：AniList disabled/403 → 明确"服务暂时不可用"提示，不中断批量；其它错误仍报网络错误。
- C：手动上传封面失败 → 保持抽屉、提示；漫画无视频路径不显示该字段；分类路径原样保留不误改。

## 测试
- A：anilist 403-disabled 识别（search_cover 对 403+disabled 响应体返回特定错误的逻辑，纯函数或 mock）。
- C：media_update/delete 参数化对 comic 表生效（造 comic 记录、update cover_path/title、断言；delete 断言删除）；import_cover 按 kind 存 covers/media 或 covers/comic。
- 前端 EditDrawer 漫画模式：tsc + 真机（文案/隐藏字段/保存写 comic）。

## 影响与风险
- 后端：anilist.rs、lib.rs（media 命令参数化 + import_cover kind）；前端：EditDrawer、ComicView、ipc。
- media_update/delete 参数化影响视频调用——视频侧传 kind="video"/表名 media，行为不变（需回归视频编辑）。
- import_cover/cropped 加 kind 影响视频调用——视频传 media，行为不变。
- **YAGNI**：不做漫画"新增"、不做 AniList 自动重试/轮询、不做备用网络源（本次只 A+C）。
