# 视频字幕渲染功能设计（JASSUB / libass-wasm）

日期：2026-09-10
状态：设计已确认，待写实现计划

## 背景与问题

播放器（`src/views/PlayerView.ts`）目前只播放视频，**完全不渲染字幕**。媒体库里每条视频在 `media.subtitle_path` 都记录了一份 `.ass` 字幕（相对 app_data 的 `subtitles/sub_xxx.ass`），但 `player_open` 命令没把它返回给前端，播放时字幕被丢弃。

字幕是完整的 **Advanced SubStation Alpha（.ass）** 格式：带 `[V4+ Styles]` 样式表（字体、字号、颜色、描边、对齐 `Alignment`、`PlayResX/Y` 分辨率基准），`Dialogue` 事件行含内联特效标签（`\blur`、`\fn` 换字体、`\fs` 改字号、`\1c` 改颜色、`\N` 换行、`\r` 重置），且普遍是**双语**（中文主行 + 英文小字副行）。字幕文件编码为 UTF-8 with BOM。引用的字体包括中文（华文楷体、微软雅黑、方正黑体简体、SimSun）与英文（Arial、Cronos Pro）。

## 目标

播放视频时，把这条视频的 `.ass` 字幕**完整还原渲染**到画面上（位置、样式、双语、特效均按字幕作者设计），并与视频时间轴精确同步；播放条上提供一个字幕开/关按钮。

## 决策汇总

| 项 | 决策 |
|----|------|
| 渲染保真度 | **完整还原 .ass**（含样式表、内联特效、双语、定位） |
| 渲染引擎 | **JASSUB**（libass 编译成 WASM，逐帧渲染到 canvas，覆盖在 `<video>` 上） |
| 字幕如何送到前端 | 扩展 `player_open` 返回值，一并返回字幕可访问 URL；前端用 `convertFileSrc` 得到 asset URL 交给 JASSUB 的 `subUrl` |
| 字体策略 | 随应用**打包一个开源 CJK 字体（Noto Sans CJK SC / 思源黑体简体）作 `defaultFont` 兜底**；字幕指名字体若系统有则 libass 自动用，找不到落到兜底字体。**跨平台（macOS + Windows）行为一致，不依赖系统恰好装了某字体。** |
| 字幕开关 | 有字幕自动显示；播放条加 CC 开关（Lucide `captions` 图标）可临时隐藏/显示 |
| CC 按钮位置 | 音量条与全屏按钮之间 |
| 无字幕时 | 不显示 CC 按钮（或禁用），不初始化 JASSUB |
| 不做（YAGNI） | 字幕字号/位置微调、多字幕轨切换、字幕上传/外挂选择、字幕搜索 |

## 架构

纯前端为主 + 一处后端小改。数据流：

```
player_open(category, category_path, title)                [后端 Rust]
  → remux 视频得到 mp4 + duration（现有逻辑不变）
  → 读该条视频 media.subtitle_path（相对 subtitles/xxx.ass）
  → 拼成 app_data 下绝对路径，返回 { src, duration, subtitle }
                                              ↑ subtitle: Option<String> 绝对路径，无字幕为 null

PlayerView（前端 TS）
  → video.src = info.src（现有）
  → 若 info.subtitle 非空：subUrl = convertFileSrc(info.subtitle)
  → new SubtitleRenderer(video, subUrl)   ← 封装 JASSUB 的新组件
  → CC 按钮 toggle → renderer.setVisible(bool)
  → 退出/切换视频 → renderer.destroy()（释放 worker/wasm）
```

时间轴同步由 JASSUB 内部完成：它持有 `<video>` 引用，监听 `timeupdate`/`seeking`/`ratechange`，按 `video.currentTime` 渲染对应帧的字幕，seek、暂停、倍速都自动跟随，**我们无需手写时间轴逻辑**。

## 组件与文件

### 新增 `src/components/SubtitleRenderer.ts`

封装 JASSUB，隔离第三方库的初始化与资源加载细节。对外接口小而清晰：

```ts
export class SubtitleRenderer {
  /** 在给定 video 上初始化字幕渲染。subUrl 为 convertFileSrc 得到的 asset URL。
   *  内部异步 await instance.ready；构造后即开始渲染。 */
  constructor(video: HTMLVideoElement, subUrl: string);
  /** 显示/隐藏字幕层（CC 开关调用）。 */
  setVisible(visible: boolean): void;
  /** 释放 JASSUB worker/wasm 与 canvas（退出播放或换片时必须调用，防泄漏）。 */
  destroy(): void;
}
```

内部实现要点：
- 用 Vite 资源导入拿 worker/wasm URL，交给 JASSUB 构造参数：
  ```ts
  import JASSUB from "jassub";
  import workerUrl from "jassub/dist/jassub-worker.js?worker&url";
  import wasmUrl from "jassub/dist/jassub-worker.wasm?url";
  import modernWasmUrl from "jassub/dist/jassub-worker-modern.wasm?url";
  ```
- CJK 兜底字体：字体文件放 `src/assets/fonts/`，用 `new URL("../assets/fonts/NotoSansCJKsc.woff2", import.meta.url).href` 得到 URL，传 `availableFonts: { "Noto Sans CJK SC": <url> }` + `defaultFont: "Noto Sans CJK SC"`。**不把兜底字体放进 `fonts[]`**（按 JASSUB 官方建议，避免 FOUT）。
- 构造：`new JASSUB({ video, subUrl, workerUrl, wasmUrl, modernWasmUrl, availableFonts, defaultFont })`。
- `setVisible`：切换 JASSUB canvas 的显隐（隐藏时设 canvas `style.display="none"` 或调用其可用的显隐 API，以库类型定义为准）。
- `destroy`：调用 JASSUB 实例的 `destroy()`（以库类型定义为准），并移除其插入的 canvas。
- 初始化失败（wasm 加载失败等）不应让播放崩溃：try/catch，失败仅 `console.error`，视频照常无字幕播放。

### 修改 `src/views/PlayerView.ts`

- 播放条 HTML 在 `.vol` 与 `.fs` 之间插入 CC 按钮：`<button class="cc" title="字幕">${icon("captions", 18)}</button>`。仅当本条有字幕时渲染/启用该按钮。
- `player_open` 成功后，若 `info.subtitle` 非空：`const sub = new SubtitleRenderer(video, convertFileSrc(info.subtitle));`，保存引用；CC 按钮初始为「开启」高亮态。
- CC 按钮 `onclick`：翻转 `subtitleOn` 布尔，调 `sub.setVisible(subtitleOn)`，并 toggle 按钮的 `.cc-on` class。
- `cleanup()`（返回、`player-detach`）中：`sub?.destroy()`。切换到另一条视频会重建 PlayerView，故无需在同实例内换轨。

### 修改 `src/lib/icons.ts`

新增 `captions` 图标（Lucide 风格，24×24，stroke currentColor，stroke-width 2）：
```ts
captions: `<rect x="3" y="5" width="18" height="14" rx="2"/><path d="M7 15h4M15 15h2M7 11h2M13 11h4"/>`,
```
并把 `"captions"` 加入 `IconName` 联合类型。

### 修改后端 `src-tauri/src/player/mod.rs`

- `PlayerInfo` 增加字段 `pub subtitle: Option<String>`（字幕绝对路径，无则 `None`）。
- `player_open` 内：视频 remux 后，用传入的 `category_path` + `title` 按 `media` 表的 `UNIQUE(category_path, title)` 查一行的 `subtitle_path`（相对路径，如 `subtitles/xxx.ass`）。`player_open` 已持有 `db: tauri::State<crate::db::Db>`，直接一句 `SELECT subtitle_path FROM media WHERE category_path=?1 AND title=?2` 即可。查到且非空则用 `library::paths::appdata_to_absolute`（已存在，见 `lib.rs:99`）转成绝对路径填入返回；查不到、字段为 NULL 或空串则 `None`。
- 不改 HTTP server（字幕走 asset 协议，`assetProtocol.enable=true` scope `**` 已允许访问 app_data 下文件）。

### 修改 `src/lib/ipc.ts`

`PlayerInfo` 前端类型（`playerOpen` 返回）增加 `subtitle: string | null`。

### 依赖与打包

- `package.json` 新增依赖 `jassub`。
- CJK 字体文件（Noto Sans CJK SC，思源黑体简体，OFL 开源许可）以 `.woff2` 放入 `src/assets/fonts/`，随前端打包（Vite 处理，`new URL(..., import.meta.url)`）。选 woff2 压缩后体积可控。
- 许可：JASSUB（MIT），Noto Sans CJK（SIL OFL 1.1），在 README 注明。

## 数据流（时序）

1. 用户点视频 → `PlayerView` → `api.playerOpen(...)`。
2. 后端 remux 视频 + 查字幕绝对路径 → 返回 `{ src, duration, subtitle }`。
3. 前端设 `video.src = src`；若 `subtitle` 非空 → `new SubtitleRenderer(video, convertFileSrc(subtitle))`。
4. JASSUB `await ready` 后持续按 `video.currentTime` 渲染字幕帧（seek/暂停/倍速自动同步）。
5. CC 按钮 toggle → `setVisible`。
6. 返回/换片 → `cleanup` → `destroy`（释放 worker/wasm/canvas）。

## 错误处理

- **无字幕**：`info.subtitle` 为 `null` → 不建 renderer、不显示 CC 按钮，视频正常播。
- **字幕文件在磁盘缺失**（坏链）：`convertFileSrc` 的 URL 请求 404 → JASSUB 内部报错，捕获后 `console.error`，视频正常播（无字幕）。当前库已核对无坏链，但需容错。
- **JASSUB 初始化失败**（wasm/worker 加载失败）：try/catch 包裹构造，失败仅记录日志，视频不受影响。
- **字体缺失**：中文一定落到打包的 Noto Sans CJK 兜底字体，不会出现方框；英文缺失字体落到 libass 默认。

## 测试策略

无前端测试框架。验证方式：

- **纯函数 / 类型**：`npx tsc --noEmit` 通过；`npm run build`（tsc + vite build）通过，确认 jassub 的 worker/wasm/字体资源被 Vite 正确打包（build 产物里能看到对应文件）。
- **后端**：`player_open` 返回 `subtitle` 字段的单元测试——在内存库插入一条带 `subtitle_path` 的 media，构造调用路径，断言返回的 `subtitle` 为对应绝对路径；无字幕条目断言为 `None`。（remux 部分不易单测，聚焦字幕字段解析。）
- **真机**（`npm run tauri dev`）：
  1. 播放一条有字幕的视频 → 字幕出现在画面底部（或字幕自定位处），中文正常不方框，双语两行显示，样式与 PotPlayer/VLC 观感一致。
  2. seek 到中段、快进快退 → 字幕时间轴跟随准确。
  3. 点 CC 按钮 → 字幕隐藏；再点 → 恢复；按钮高亮态正确切换。
  4. 返回列表再进另一条视频 → 无残留字幕、无 worker 泄漏（多次进出仍正常）。
  5. 播放一条**无字幕**视频 → 不显示 CC 按钮，视频正常播。
  6. Windows 上重复 1、3（跨平台字体验证）——若当前无 Windows 环境，至少在 spec/README 标注需回归。

## 明确不做（YAGNI）

- 字幕字号缩放 / 垂直位置微调。
- 多字幕轨切换、字幕语言选择。
- 播放中动态更换字幕文件 / 外挂字幕选择。
- 字幕内容搜索 / 跳转。
- MKV 内嵌字幕轨提取（当前字幕都是独立 .ass 文件）。

## 风险

- **JASSUB 资源在 Tauri WKWebView + Vite 下的加载**：worker/wasm 用 `?url` 导入应能被 Vite 正确产出并经 asset 协议加载；CSP 为 `null`（宽松）不拦截。真机第一步即验证；若 worker 加载有问题，回退用主线程模式（JASSUB 支持无 SharedArrayBuffer 时单线程运行）。
- **兜底字体体积**：Noto Sans CJK SC 全量 woff2 仍有数 MB。可接受（桌面应用）；若过大，后续可换子集化字体，但本次不做子集化（YAGNI）。
- **libass 特效开销**：逐帧 canvas 渲染对复杂特效字幕有 CPU 开销，一般视频可接受；真机步骤 2 观察卡顿。
