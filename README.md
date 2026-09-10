# Collector

Collector 是一款跨平台（macOS + Windows）的本地素材管理器，用于集中管理并一站式查看/运行三类本地素材：

- **视频**：MKV 影片，按「电影 / 动漫 / 电视剧」分类。
- **漫画**：zip 图片包。
- **游戏**：绿色版（免安装）游戏。

它提供两条能力线：

1. **素材标准化**：字幕批量校准（.ass 中英合并、中英标点各自规范、特殊字符/OCR 纠错、统一样式与分类，附质检报告）、漫画打包（图片自然序重命名并打成 zip）。
2. **一站式查看**：内嵌 mpv 视频播放器、漫画阅读器、游戏启动器。

技术栈：Tauri 2.x（Rust 后端 + WebView 前端，vanilla-ts）、SQLite 索引、libmpv 播放、玻璃拟态 UI。

## 功能概览

- 三类素材统一扫描、索引（SQLite）与浏览。
- 视频：内嵌 mpv 播放，支持外挂 `.ass` 字幕、封面、简介侧载。
- 漫画：zip 包内置阅读器，自动提取首图作为封面。
- 游戏：按平台启动绿色版游戏；当前平台无对应可执行文件时仍显示但标注「本平台不可用」。
- 独立的**标准化工具台**：字幕批量校准 + 漫画标准化，附质检报告。

## 开发依赖

- **Node.js**（前端构建 / Tauri CLI）
- **Rust**（后端）
- **mpv**（提供 libmpv 播放能力）
  - macOS：`brew install mpv`
  - Windows：需自备 `libmpv-2.dll` 及对应 SDK（参见「平台可移植性」）

## 快速开始

```bash
# 安装前端依赖
npm install

# 开发模式（热重载）
npm run tauri dev

# 构建生产包
npm run tauri build
```

> macOS 上 `npm run tauri build` 之后还需运行 `scripts/relocate-libmpv.sh` 重定位 libmpv，详见下文「macOS 打包（libmpv 随包分发）」。

## 素材根目录约定

在应用「设置」中分别配置三个根目录，扫描后按以下约定识别素材。

### 视频

```
<视频根>/<分类>/.../<剧名>/*.mkv
```

- **分类**取第一级目录名（电影 / 动漫 / 电视剧），支持多级子目录递归。
- 侧载文件（放在与 `.mkv` 同目录）：
  - 同名 `.ass`：外挂字幕
  - `poster.jpg` 或同名 `.jpg`：封面
  - `info.txt`：简介

### 漫画

```
<漫画根>/<分类>/.../<漫画名>.zip
```

- zip 内含 jpg / png 图片。
- 封面自动取 zip 内的首张图片。
- 同名 `.txt`：简介。

### 游戏

```
<游戏根>/<游戏名>/
```

每个游戏目录内放一个 `game.json`。封面为同目录的 `cover.jpg`。

`game.json` 字段：

| 字段          | 类型            | 必填 | 说明                                                        |
| ------------- | --------------- | ---- | ----------------------------------------------------------- |
| `name`        | string          | 是   | 显示名                                                      |
| `description` | string          | 否   | 简介；缺失时读取同目录 `info.txt`                           |
| `exec_win`    | string          | 否   | Windows 启动程序相对路径                                    |
| `exec_mac`    | string          | 否   | macOS 启动程序相对路径（如 `Xxx.app`）                      |
| `fullscreen`  | bool            | 否   | 是否全屏启动                                                |

> 当前平台无对应 `exec` 的游戏仍会在列表中显示，但标注「本平台不可用」。

## 标准化工具台

独立菜单，提供两类素材标准化工具。

### 字幕批量校准

选择输入目录（可配置、自动记住；默认 `docs/subtitles`），递归校准目录下所有 `.ass` 字幕，**输出固定写入应用字幕库**（`<应用数据目录>/subtitles/<分类>/<相对目录>/<标题>.ass`，与播放器所用字幕同一份文件），处理完附质检报告。

校准遵循一条核心原则：**能确定的自动修改，有语义判断风险的只在结果列表提示、不擅自臆改**。

#### 自动执行的规则（会改写字幕）

1. **编码兼容**：自动识别并解码 UTF-8（含 BOM）、UTF-16（带 BOM）、GBK；无法识别的文件跳过并在报告中提示，不中断整批。
2. **跳过空文本行**：仅含特效标签（`{\pos..}`、`{\shad1}` 等）、`\N`、空白而无实际文字的 `Dialogue` 行直接剔除，不输出、不参与检测。
3. **中英合并**：开始与结束时间**完全相同**的多条 `Dialogue`（一中一英）合并为一条「中文 + 英文」双语字幕；同一条内已有的 `\N{...}` 分隔统一为标准分隔符。
4. **中文标点体系**（仅作用于中文段）：半角标点转全角——`, → ，`、`! → ！`、`? → ？`、`: → ：`、`... → …`、`. → 。`；成对英文引号 `"` 按奇偶配对转为 `「` `」`。
5. **英文标点体系**（仅作用于英文段）：标点前多余空格移除、逗号后补空格、多空格并一；行尾若以字母/数字结尾补英文句号。中英两段严格分开处理，互不串扰。
6. **特殊字符 / OCR 纠错**（数据表驱动，可扩展）：`'' → "`、`-- → …`、连续 2+ 个点 → `…`、多空格 → 单空格；常见 OCR 误识别修正（如独立的 `l`/`i` → `I`、`lt' → It'`、`lsn' → Isn'`）。
7. **对话破折号规整**：把已标注的对话破折号统一为「`- `」（破折号后恰一个空格），中英各自对齐；**不臆测**某句是否为对话。
8. **已标注标记规整**：中文段圆括号 `()` 全角化为 `（）`并去内侧多余空格；方括号 `[]`（外语）、井号 `#`（歌曲）保留并去内侧多余空格。
9. **统一样式与分类**：输出统一的 ASS 样式头（`Default` / `Title` / `Note` 三样式）；按字符标志分类——含书名号 `《》` → `Title`（片名），被 `（）` 整条括起 → `Note`（注释），其余 → `Default`。含英文的行输出中英双行、纯中文行输出单行。

#### 仅提示、不自动修改（列在结果列表，人工核对）

- **疑似未合并中英**：相邻两条时间轴很接近（开始、结束差都 ≤ 0.5 秒）且一中一英，疑似本应合并却没对齐。
- **时间轴交叉**：按时间排序后，某条与其前后各 3 条存在区间重叠（时间轴**完全相同**的中英对属正常，不报）。
- **同时间轴多于 2 条**：同一时间轴出现 3 条以上，只取中/英各一条，其余需人工确认。
- **残留可疑标点**：`,.`、连续空格等；`.,` 仅在点号前不是字母时才提示（英文缩写如 `D.C.,`、`Jan.,` 属正常，不报）。
- **读取失败**：编码无法识别的文件。

结果列表按文件分组、可折叠，每条提示带类型色标、行号与文本摘要。

> 语义判断类内容（哪句是对话、哪句是道具翻译 / 外语 / 歌词）不做自动识别，避免误伤正常字幕——引擎只规整你已经标注好的，并把可疑之处挑出来供人工处理。

### 漫画标准化

选择图片目录、命名前缀与输出 zip 路径：

- 图片按自然序重命名为 `前缀_001.jpg`、`前缀_002.jpg`……
- 重命名后打包为 zip

## macOS 打包（libmpv 随包分发）

Collector 依赖 `libmpv`。为了让 `.app` 分发到**未安装 mpv** 的 macOS 机器上仍能运行，
需要把 `libmpv.2.dylib` 打进 app 并让可执行文件在 app 内部定位到它。

已配置的部分（自动生效）：

1. **`src-tauri/tauri.conf.json` → `bundle.macOS.frameworks`**：把
   `/opt/homebrew/Cellar/mpv/0.41.0_9/lib/libmpv.2.dylib` 拷进
   `Collector.app/Contents/Frameworks/`。（指向 Cellar 里的真实文件，而非符号链接。
   升级 mpv 后需更新此版本号路径。）
2. **`src-tauri/build.rs`**：向可执行文件注入 rpath
   `@executable_path/../Frameworks`，使其能在 app 内的 Frameworks 目录搜索库。

必须手动补充的部分（**当前未自动化**）：

`libmpv.2.dylib` 自身的 `install_name` 是绝对路径
`/opt/homebrew/opt/mpv/lib/libmpv.2.dylib`，所以 `tauri build` 产出的可执行文件
load command 记录的也是这个绝对路径 —— **@rpath 不会被使用**，在没装 mpv 的机器上
仍会报库找不到。必须在打包后运行重定位脚本把引用改成 `@rpath/libmpv.2.dylib`：

```bash
npm run tauri build            # 产出 Collector.app（DMG 步骤在本机可能失败，可忽略）
./scripts/relocate-libmpv.sh   # 后处理：install_name_tool 重写引用为 @rpath
```

`scripts/relocate-libmpv.sh` 会：
- `install_name_tool -id @rpath/libmpv.2.dylib <Frameworks/libmpv.2.dylib>`
- `install_name_tool -change /opt/homebrew/opt/mpv/lib/libmpv.2.dylib @rpath/libmpv.2.dylib <MacOS/Collector>`

> 注意：`install_name_tool` 会使代码签名失效。正式分发时需在此步骤之后**重新签名并公证**
> （`codesign --force --deep --sign ...`），再打 DMG。CI 应把
> `build → relocate-libmpv.sh → codesign → notarize → dmg` 串成流水线。

### 验证状态（诚实标注）

- ✅ `.app` 能产出；`Contents/Frameworks/libmpv.2.dylib` 存在；rpath 已注入。
- ⚠️ `tauri build` 直接产物的可执行文件对 libmpv 的引用**仍是绝对路径**，@rpath 未生效。
- ✅ `relocate-libmpv.sh` 的 `install_name_tool` 重写已在产物副本上验证：
  引用成功改为 `@rpath/libmpv.2.dylib`。
- ❌ **未在干净（无 mpv）的机器上实测运行**——理论上重定位后应可加载，但本环境未做端到端验证。

## 平台可移植性（`src-tauri/.cargo/config.toml`）

`.cargo/config.toml` 的 `-L` 链接搜索路径是**本机 Homebrew 安装位置**专用：

- Apple Silicon（`aarch64-apple-darwin`）：`/opt/homebrew/lib`
- Intel（`x86_64-apple-darwin`）：`/usr/local/lib`
- Windows：`libmpv` 以 `libmpv-2.dll` 分发，需另配 `-L` 指向 SDK 的 lib 目录，
  并把 DLL 作为 resource 打进包（**本仓库暂无 Windows 环境，未实现**）。

CI / 异构环境建议改为在 `build.rs` 里用 `brew --prefix mpv` 动态探测路径并 emit
`cargo:rustc-link-search`，而非硬编码。Intel 机器上打包同样需要把
`relocate-libmpv.sh` 里的 `OLD_REF` 改为 `/usr/local/opt/mpv/lib/libmpv.2.dylib`。
