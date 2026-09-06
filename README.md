# Collector

Collector 是一款跨平台（macOS + Windows）的本地素材管理器，用于集中管理并一站式查看/运行三类本地素材：

- **视频**：MKV 影片，按「电影 / 动漫 / 电视剧」分类。
- **漫画**：zip 图片包。
- **游戏**：绿色版（免安装）游戏。

它提供两条能力线：

1. **素材标准化**：字幕清洗（.ass 中英双语合并、标点全角化、统一样式头）、漫画打包（图片自然序重命名并打成 zip）。
2. **一站式查看**：内嵌 mpv 视频播放器、漫画阅读器、游戏启动器。

技术栈：Tauri 2.x（Rust 后端 + WebView 前端，vanilla-ts）、SQLite 索引、libmpv 播放、玻璃拟态 UI。

## 功能概览

- 三类素材统一扫描、索引（SQLite）与浏览。
- 视频：内嵌 mpv 播放，支持外挂 `.ass` 字幕、封面、简介侧载。
- 漫画：zip 包内置阅读器，自动提取首图作为封面。
- 游戏：按平台启动绿色版游戏；当前平台无对应可执行文件时仍显示但标注「本平台不可用」。
- 独立的**标准化工具台**：字幕标准化 + 漫画标准化，附质检报告。

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

### 字幕标准化

选择输入目录与输出目录，递归清洗 `.ass` 字幕：

- 中英双语合并
- 标点全角化
- 统一 ASS 样式头

处理完成后附质检报告。

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
