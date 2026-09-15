# ffmpeg 缺失检测提示 + Windows 安装时可选下载 + 清理 · 设计

日期：2026-09-15
分支：master

## 背景与目标

排查确认：Collector 视频播放的真实依赖是 **ffmpeg/ffprobe**（外部命令，`transcode.rs` 用 `-c:v copy` + `-c:a aac` remux）。当前打包版未捆绑 ffmpeg，靠系统 PATH——无 ffmpeg 的机器视频不可用，且失败时错误信息含糊（`"ffmpeg spawn: ..."`）。

评估捆绑方案后（GPL/LGPL licensing、arm64 静态构建源稀缺），选定**运行时策略：检测 + 详细提示**——不在应用内捆绑 ffmpeg，缺失时给出清晰、可操作的引导（问题定位 + 各平台安装方法 + 下载链接）。**此外**，在 Windows 安装器上增加「知情同意 + 架构适配」的可选下载（用户勾选式确认后才下载），作为 Windows 的便利增强，其余平台靠运行时提示。

## 目标

1. **ffmpeg 缺失时给详细提示**（全平台运行时兜底）：转码/探测因找不到 ffmpeg 失败时，返回包含问题定位、解决方案、各平台安装命令与下载链接的错误信息，前端清晰展示。
2. **Windows 安装时可选下载**：NSIS 安装 hook 检测系统 ffmpeg，缺失则弹确认框征得用户同意，同意后按 x64/arm64 架构下载对应的 BtbN 最新构建到 `bin\`。
3. **清理**：清本地中间产物（/tmp 图标临时文件、release bundle）。

## 涉及文件

- `src-tauri/src/player/transcode.rs`（ffmpeg 检测 + 详细错误）
- `src-tauri/installer-hooks.nsi`（升级为确认框 + 架构适配下载）
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
         · Windows：重新运行安装程序并在提示时选择「是」自动下载；或手动从 \
         https://github.com/BtbN/FFmpeg-Builds/releases 下载 win64/winarm64 gpl zip，\
         解压后把 ffmpeg.exe/ffprobe.exe 放到 Collector 安装目录的 bin 文件夹（或加入系统 PATH）。\n\
         · Linux：用发行版包管理器安装，如  sudo apt install ffmpeg\n\n\
         安装后重启 Collector 即可。也可把 ffmpeg/ffprobe 放到 Collector 可执行文件旁的 bin 目录。"
    )
}
```

在 `remux` 开头（`probe_duration` 之前）调用 `ensure_ffmpeg_available()?;`——这样缺失时立即返回详细提示，而非等到 spawn 报含糊错误。`probe_duration` 内部保持原逻辑（remux 已先检测，probe 单独调用场景少）。

**前端**：`PlayerView.ts` 的 catch 已 `loading.textContent = "无法播放该视频：" + e`。因错误信息现在含换行的详细文本，改为用 `white-space: pre-line` 或 `<pre>` 展示，保留换行可读性。具体：把 loading 容器加 `style="white-space:pre-line;text-align:left;max-width:560px"`（或等效），使多行提示正常换行显示。

**验收**：在无 ffmpeg 的环境（或临时改 PATH）打开视频，显示的是包含定位+安装命令+链接的多行提示，而非 `"ffmpeg spawn: No such file"`；有 ffmpeg 时播放正常。

---

## 2 · Windows 安装时可选下载 ffmpeg（知情同意 + 架构适配）

**现状**：`installer-hooks.nsi` 在 Windows 安装时**静默**检测 ffmpeg、缺失则自动从 BtbN 下载到 `$INSTDIR\bin`（仅 win64、用 `Expand-Archive`）。问题：静默下载（无用户同意）、只适配 x64、原用 BtbN 的 win64 链接但格式假设不够稳。

**改法**：把 hook 升级为「**弹确认框征得同意 + 按系统架构适配下载**」：

1. **检测**：`NSIS_HOOK_POSTINSTALL` 里先 `ffmpeg -version` 检测系统是否已有 ffmpeg。已有则跳过（不打扰）。
2. **确认框**：缺失时用 NSIS `MessageBox MB_YESNO` 弹出：「检测到未安装 ffmpeg。Collector 的视频播放需要它。是否现在自动下载安装？（约数十 MB，需联网几分钟）点『否』可稍后手动安装。」用户点「是」才继续下载；点「否」跳过（运行时仍有第 1 节的详细提示兜底）。
3. **架构适配**：读安装环境架构选对应 BtbN 包：
   - 检测 arm64：读 `PROCESSOR_ARCHITECTURE` / `PROCESSOR_ARCHITEW6432`，值为 `ARM64` 则用 arm64 包，否则用 x64 包。
   - x64：`https://github.com/BtbN/FFmpeg-Builds/releases/download/latest/ffmpeg-master-latest-win64-gpl.zip`
   - arm64：`https://github.com/BtbN/FFmpeg-Builds/releases/download/latest/ffmpeg-master-latest-winarm64-gpl.zip`
   （BtbN 用 `latest` 稳定 URL，始终最新构建，无需维护版本号；两架构均为 zip，`Expand-Archive` 原生支持。）
4. **下载/解压/放置**：PowerShell `Invoke-WebRequest` 下载 zip → `Expand-Archive` → 递归找 `ffmpeg.exe`/`ffprobe.exe` 拷到 `$INSTDIR\bin` → 清理临时文件。与原 hook 的机制一致，仅加架构分支与确认框。
5. **失败兜底**：下载失败 `DetailPrint` 提示可稍后手动安装，不中断安装。

**保留** `installer-hooks.nsi` 与 `tauri.conf.json` 的 `bundle.windows.nsis.installerHooks` 配置（不再删除——改为升级）。

**licensing 说明**：下载的是 BtbN GPL 构建。因是**用户在自己设备上主动同意后下载**（非随 Collector 分发），ffmpeg 二进制不进 Collector 安装包，缓解 GPL 传染顾虑；确认框措辞明确告知这是下载第三方组件。

**平台差异**：此机制仅 Windows（有 NSIS 安装器）。macOS（dmg 拖拽，无安装器）与 Linux 仍靠第 1 节的「检测 + 详细提示」，用户手动安装。三平台策略：Windows 安装时可选便利下载 + 全平台运行时详细提示兜底。

**验收**：Windows 安装时若无 ffmpeg，弹确认框；点「是」按 x64/arm64 下载正确包到 `bin\`；点「否」跳过且不报错；已有 ffmpeg 时不弹框。`ffmpeg_bin` 的 `<exe>/bin/` 查找能定位到下载的二进制。

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

- 不在**应用内**捆绑 ffmpeg（不进 .app/exe 包体；方案「打包捆绑」与「自建 LGPL 静态构建」已否决：licensing + arm64 LGPL 源稀缺）。Windows 安装器的可选下载是用户同意后从第三方源获取，不算应用内捆绑。
- macOS/Linux 不做自动下载（无安装器 / 保持简单），靠运行时详细提示。
- 不改 `-c:v copy` + `-c:a aac` 的转码逻辑。
- 不删 `ffmpeg_bin` 的 bin/ 查找（保留便携放置能力）。
- 不清 `.superpowers/brainstorm`（保留设计预览）。
