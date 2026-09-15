# ffmpeg 缺失检测 + 详细提示 + 清理遗留 · 设计

日期：2026-09-15
分支：master

## 背景与目标

排查确认：Collector 视频播放的真实依赖是 **ffmpeg/ffprobe**（外部命令，`transcode.rs` 用 `-c:v copy` + `-c:a aac` remux）。当前打包版未捆绑 ffmpeg，靠系统 PATH——无 ffmpeg 的机器视频不可用，且失败时错误信息含糊（`"ffmpeg spawn: ..."`）。

评估捆绑方案后（GPL/LGPL licensing、arm64 静态构建源稀缺），选定**方案 3：检测 + 详细提示**——不捆绑、不下载 ffmpeg，而是在缺失时给出清晰、可操作的引导（问题定位 + 各平台安装方法 + 下载链接）。同时统一三平台策略，清理为「下载 ffmpeg」而存在的遗留（Windows NSIS hook）。

## 目标

1. **ffmpeg 缺失时给详细提示**：转码/探测因找不到 ffmpeg 失败时，返回包含问题定位、解决方案、各平台安装命令与下载链接的错误信息，前端清晰展示。
2. **统一策略**：三平台一致为「检测系统 ffmpeg，缺失则提示」，删除 Windows 的 NSIS 下载 hook。
3. **清理**：删 NSIS hook 及 tauri.conf 引用；清本地中间产物（/tmp 图标临时文件、release bundle）。

## 涉及文件

- `src-tauri/src/player/transcode.rs`（ffmpeg 检测 + 详细错误）
- `src-tauri/tauri.conf.json`（删 nsis.installerHooks 配置）
- `src-tauri/installer-hooks.nsi`（删除）
- `src/views/PlayerView.ts`（错误信息展示格式化，可选）
- README.md（若提及 ffmpeg 安装，补充/对齐）

---

## 1 · ffmpeg 检测与详细提示（后端）

**现状**：`transcode.rs` 的 `probe_duration` / `remux` 直接 `Command::new(ffmpeg_bin(...))`，spawn 失败返回 `AppError::Other("ffprobe spawn: {e}")` / `("ffmpeg spawn: {e}")`——含糊，用户不知道是缺 ffmpeg 还是别的。

**改法**：新增一个检测 + 详细提示的辅助。在 `transcode.rs` 加：

```rust
/// 检测 ffmpeg/ffprobe 是否可用（能否 spawn）。返回 Ok(()) 或含详细引导的 Err。
fn ensure_ffmpeg_available() -> AppResult<()> {
    for tool in ["ffmpeg", "ffprobe"] {
        let ok = Command::new(ffmpeg_bin(tool))
            .arg("-version")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false);
        if !ok {
            return Err(AppError::Other(ffmpeg_missing_hint(tool)));
        }
    }
    Ok(())
}

/// 生成 ffmpeg 缺失时的详细中文引导（问题定位 + 各平台安装/下载）。
fn ffmpeg_missing_hint(tool: &str) -> String {
    format!(
        "视频播放需要 {tool}，但未在系统中找到。\n\n\
         【问题定位】Collector 用 ffmpeg/ffprobe 将视频无损转封装为浏览器可播放的 MP4，\
         未安装或不在 PATH 时无法播放。\n\n\
         【解决方案】安装 ffmpeg（含 ffmpeg 与 ffprobe）：\n\
         · macOS：终端运行  brew install ffmpeg\n\
         · Windows：下载 https://www.gyan.dev/ffmpeg/builds/ 的 release-full 包，\
         解压后将 bin 目录加入系统 PATH；或把 ffmpeg.exe/ffprobe.exe 放到 Collector 安装目录的 bin 文件夹。\n\
         · Linux：用发行版包管理器安装，如  sudo apt install ffmpeg\n\n\
         安装后重启 Collector 即可。也可把 ffmpeg/ffprobe 放到 Collector 可执行文件旁的 bin 目录。"
    )
}
```

在 `remux` 开头（`probe_duration` 之前）调用 `ensure_ffmpeg_available()?;`——这样缺失时立即返回详细提示，而非等到 spawn 报含糊错误。`probe_duration` 内部保持原逻辑（remux 已先检测，probe 单独调用场景少）。

**前端**：`PlayerView.ts` 的 catch 已 `loading.textContent = "无法播放该视频：" + e`。因错误信息现在含换行的详细文本，改为用 `white-space: pre-line` 或 `<pre>` 展示，保留换行可读性。具体：把 loading 容器加 `style="white-space:pre-line;text-align:left;max-width:560px"`（或等效），使多行提示正常换行显示。

**验收**：在无 ffmpeg 的环境（或临时改 PATH）打开视频，显示的是包含定位+安装命令+链接的多行提示，而非 `"ffmpeg spawn: No such file"`；有 ffmpeg 时播放正常。

---

## 2 · 统一策略：删除 Windows NSIS 下载 hook

**现状**：`installer-hooks.nsi` 在 Windows 安装时检测 ffmpeg、缺失则用 PowerShell 从 BtbN 下载 GPL 静态构建到 `$INSTDIR\bin`。这与方案 3「检测+提示」不一致（Windows 自动下载、其他平台提示），且引入 GPL 二进制分发的 licensing 牵涉。

**改法**：
- 删除文件 `src-tauri/installer-hooks.nsi`。
- 删除 `tauri.conf.json` 中 `bundle.windows.nsis.installerHooks` 配置（若删后 `bundle.windows` 变空对象，一并删 `windows` 键）。
- Windows 用户改由第 1 节的详细提示引导手动安装（提示里已含 Windows 下载链接与放置说明）。

**保留**：`ffmpeg_bin` 的 `<exe>/bin/` 查找逻辑保留——用户仍可手动把 ffmpeg 放到 app 旁的 bin 目录（便携方式），只是不再自动下载。更新其注释：去掉「Windows NSIS 安装时把 ffmpeg 下载到此」的描述，改为「用户可手动将 ffmpeg 放于此」。

**验收**：`tauri.conf.json` 无 nsis 配置、`installer-hooks.nsi` 不存在；`ffmpeg_bin` 注释准确；cargo build 通过。

---

## 3 · 清理本地中间产物

（本地磁盘清理，不涉及 git——这些文件本就不在版本控制中）

- 删 `/tmp/icon_src_512.png`、`/tmp/icon_src_1024.png`、`/tmp/icon_preview.png`（图标生成临时文件）。
- 删 `/tmp/icons_backup_*`（换图标时的旧图标备份，图标已确认无误，备份可弃）。
- 删 `src-tauri/target/release/bundle`（打包产物 .app/dmg，下次 build 重生，不进 git）。

**保留**：`.superpowers/brainstorm`（可视化设计预览，留作回顾）。

**验收**：上述 /tmp 与 bundle 路径清空；仓库内容不受影响（git status 不因清理产生变化）。

---

## 测试策略

- 后端：`cargo test --lib` 保持通过（新增的检测/提示函数是纯逻辑，可加一条测试验证 `ffmpeg_missing_hint` 含关键引导词，或依赖手动验证）。
- 前端：`tsc --noEmit` + 手动验证——有/无 ffmpeg 两种情况下播放行为与提示。
- 无 ffmpeg 验证方法：临时重命名系统 ffmpeg 或改 PATH，确认提示正确、多行可读。

## 非目标（YAGNI）

- 不捆绑、不下载 ffmpeg（方案 1/2 已否决：licensing + arm64 LGPL 源稀缺）。
- 不改 `-c:v copy` + `-c:a aac` 的转码逻辑。
- 不删 `ffmpeg_bin` 的 bin/ 查找（保留便携放置能力）。
- 不清 `.superpowers/brainstorm`（保留设计预览）。
