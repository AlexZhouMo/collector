# ffmpeg 检测提示 + Windows 安装时可选下载 + 清理 实现计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** ffmpeg 缺失时给全平台详细运行时提示；Windows 安装时经用户同意、按架构适配下载 ffmpeg；清理本地中间产物。

**Architecture:** 后端 `transcode.rs` 加 ffmpeg 可用性检测 + 详细中文提示；前端播放错误多行可读展示；Windows NSIS hook 升级为「确认框 + x64/arm64 适配下载 BtbN latest」；本地清 /tmp 图标临时文件与 release bundle。工作在 master 直接进行。

**Tech Stack:** Rust（Command 检测）、TypeScript（vanilla-ts）、NSIS（PowerShell 下载）。

**验证局限说明**：NSIS hook 改动无法在 macOS 本机端到端验证（需 Windows 安装器环境）——本计划对 NSIS 部分靠脚本逻辑审查 + 语法自检，运行验证留待 Windows 打包时。后端/前端可正常验证。

---

## Task 1: 后端 ffmpeg 检测 + 详细提示

**Files:**
- Modify: `src-tauri/src/player/transcode.rs`

- [ ] **Step 1: 加检测函数与提示生成函数**

在 `transcode.rs` 中 `ffmpeg_bin` 函数之后、`probe_duration` 之前，新增：

```rust
/// 检测 ffmpeg 与 ffprobe 是否可用（能 spawn 且 -version 成功）。
/// 缺失时返回含详细引导的 Err，供前端展示。
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

/// ffmpeg/ffprobe 缺失时的详细中文引导（问题定位 + 各平台安装/下载）。
fn ffmpeg_missing_hint(tool: &str) -> String {
    format!(
        "视频播放需要 {tool}，但未在系统中找到。\n\n\
         【问题定位】Collector 用 ffmpeg/ffprobe 将视频无损转封装为浏览器可播放的 MP4，\
         未安装或不在 PATH 时无法播放。\n\n\
         【解决方案】安装 ffmpeg（含 ffmpeg 与 ffprobe）：\n\
         · macOS：终端运行  brew install ffmpeg\n\
         · Windows：重新运行安装程序并在提示时选择「是」自动下载；\
         或从 https://github.com/BtbN/FFmpeg-Builds/releases 下载 win64/winarm64 gpl zip，\
         解压后把 ffmpeg.exe/ffprobe.exe 放到 Collector 安装目录的 bin 文件夹（或加入系统 PATH）。\n\
         · Linux：用发行版包管理器安装，如  sudo apt install ffmpeg\n\n\
         安装后重启 Collector 即可。也可把 ffmpeg/ffprobe 放到 Collector 可执行文件旁的 bin 目录。"
    )
}
```

- [ ] **Step 2: remux 开头调用检测**

在 `remux` 函数体开头（`std::fs::create_dir_all(cache_dir).ok();` 之前或紧随，`probe_duration` 之前）加：

```rust
    ensure_ffmpeg_available()?;
```

放在 `pub fn remux(cache_dir: &Path, path: &str) -> AppResult<(String, f64)> {` 之后第一行最稳妥。这样 ffmpeg 缺失时立即返回详细提示，不再走到含糊的 spawn 错误。

- [ ] **Step 3: 加单元测试验证提示含关键引导词**

在 `transcode.rs` 的 `#[cfg(test)] mod tests` 中新增：

```rust
    #[test]
    fn ffmpeg_hint_contains_guidance() {
        let h = ffmpeg_missing_hint("ffmpeg");
        assert!(h.contains("brew install ffmpeg"));
        assert!(h.contains("BtbN"));
        assert!(h.contains("问题定位"));
        assert!(h.contains("解决方案"));
    }
```

- [ ] **Step 4: 编译 + 测试**

Run: `cd /Users/zhoumo/Documents/Claude/collector/src-tauri && cargo build 2>&1 | tail -8 && cargo test --lib 2>&1 | tail -8`
Expected: 编译通过；测试全绿（含新增 `ffmpeg_hint_contains_guidance`）。

- [ ] **Step 5: 提交**

```bash
git add src-tauri/src/player/transcode.rs
git commit -m "feat(player): ffmpeg 缺失时返回详细可操作的中文提示"
```

---

## Task 2: 前端播放错误多行可读展示

**Files:**
- Modify: `src/views/PlayerView.ts`

- [ ] **Step 1: 让 loading/错误容器保留换行**

`PlayerView.ts` 的错误显示走 `loading.textContent = "无法播放该视频：" + e`（catch 分支）和 video error 分支。因错误文本现在含多行（\n），需让 `.player-loading` 容器保留换行且左对齐、限宽。

读 `PlayerView.ts`，找到 `.player-loading` 元素的创建/样式处。给它加内联样式（或在错误时设置）：把展示错误的元素设为 `white-space:pre-line; text-align:left; max-width:560px; line-height:1.6; margin:0 auto`。

具体实现：在 catch 分支设置文本时一并加样式：

```typescript
  } catch (e) {
    loading.style.whiteSpace = "pre-line";
    loading.style.textAlign = "left";
    loading.style.maxWidth = "560px";
    loading.style.lineHeight = "1.6";
    loading.textContent = "无法播放该视频：\n\n" + e;
    console.error("[player] playerOpen failed", e);
  }
```

（`e` 是后端返回的 AppError 字符串，已含详细多行提示。前缀加换行让「无法播放」与详情分隔。）

- [ ] **Step 2: 类型检查**

Run: `cd /Users/zhoumo/Documents/Claude/collector && npx tsc --noEmit 2>&1 | tail -5`
Expected: 无错误。

- [ ] **Step 3: 提交**

```bash
git add src/views/PlayerView.ts
git commit -m "feat(player): 播放错误多行可读展示（保留 ffmpeg 提示换行）"
```

---

## Task 3: Windows NSIS hook 升级为确认框 + 架构适配下载

**Files:**
- Modify: `src-tauri/installer-hooks.nsi`

- [ ] **Step 1: 用确认框 + 架构适配重写 POSTINSTALL hook**

将 `src-tauri/installer-hooks.nsi` 的内容替换为：

```nsis
; NSIS 安装钩子：检测 ffmpeg，未装则征得用户同意后按架构下载 BtbN 静态构建到 $INSTDIR\bin
; 由 tauri.conf.json 的 bundle.windows.nsis.installerHooks 引用。
; 用 PowerShell 下载/解压，不依赖第三方 NSIS 插件（Tauri 自带 NSIS 无 inetc）。

!macro NSIS_HOOK_POSTINSTALL
  ; 1. 检测系统 PATH 里是否已有 ffmpeg
  nsExec::ExecToStack 'cmd /c ffmpeg -version'
  Pop $0 ; 退出码（0=已装）
  ${If} $0 != 0
    ; 2. 征得用户同意
    MessageBox MB_YESNO|MB_ICONQUESTION "检测到未安装 ffmpeg。$\r$\nCollector 的视频播放需要它（约数十 MB，需联网下载几分钟）。$\r$\n$\r$\n是否现在自动下载安装？$\r$\n点『否』可稍后手动安装。" IDYES ffmpeg_yes IDNO ffmpeg_no
    ffmpeg_yes:
      ; 3. 按架构选 BtbN 包（PROCESSOR_ARCHITECTURE=ARM64 → arm64，否则 x64）
      StrCpy $2 "win64"
      ${If} $PROCESSOR_ARCHITECTURE == "ARM64"
        StrCpy $2 "winarm64"
      ${EndIf}
      ; PROCESSOR_ARCHITEW6432 兜底（WOW64 下 32 位安装器读到的可能不准）
      ReadEnvStr $3 "PROCESSOR_ARCHITEW6432"
      ${If} $3 == "ARM64"
        StrCpy $2 "winarm64"
      ${EndIf}
      DetailPrint "正在为 $2 下载 ffmpeg（可能需要几分钟）..."
      CreateDirectory "$INSTDIR\bin"
      nsExec::ExecToLog 'powershell -NoProfile -Command "$ProgressPreference=''SilentlyContinue''; try { Invoke-WebRequest -Uri ''https://github.com/BtbN/FFmpeg-Builds/releases/download/latest/ffmpeg-master-latest-$2-gpl.zip'' -OutFile ''$INSTDIR\ffmpeg.zip'' } catch { exit 1 }"'
      Pop $1 ; 下载退出码
      ${If} $1 == 0
        nsExec::ExecToLog 'powershell -NoProfile -Command "Expand-Archive -Force ''$INSTDIR\ffmpeg.zip'' ''$INSTDIR\ffmpeg_tmp''"'
        nsExec::ExecToLog 'powershell -NoProfile -Command "Get-ChildItem -Recurse ''$INSTDIR\ffmpeg_tmp'' -Include ffmpeg.exe,ffprobe.exe | ForEach-Object { Copy-Item $_.FullName ''$INSTDIR\bin'' -Force }"'
        Delete "$INSTDIR\ffmpeg.zip"
        RMDir /r "$INSTDIR\ffmpeg_tmp"
        DetailPrint "ffmpeg 已安装到 $INSTDIR\bin"
      ${Else}
        DetailPrint "ffmpeg 下载失败，视频转码功能将不可用。可稍后手动安装 ffmpeg 或将其加入 PATH。"
      ${EndIf}
      Goto ffmpeg_done
    ffmpeg_no:
      DetailPrint "已跳过 ffmpeg 下载。视频播放需要 ffmpeg，可稍后手动安装或将其加入 PATH。"
    ffmpeg_done:
  ${EndIf}
!macroend
```

关键点说明（供实现者理解，不必写进文件）：
- `$2` 存架构标识（win64/winarm64），拼进下载 URL。
- `MessageBox MB_YESNO ... IDYES ffmpeg_yes IDNO ffmpeg_no`：点是跳 `ffmpeg_yes` 标签，点否跳 `ffmpeg_no`。
- `$PROCESSOR_ARCHITECTURE` 是 NSIS 内置只读变量；`PROCESSOR_ARCHITEW6432` 用 `ReadEnvStr` 读环境变量兜底。
- zip 用 `Expand-Archive`（BtbN Windows 包是 zip）。

- [ ] **Step 2: 确认 tauri.conf 仍引用该 hook（保留，不删）**

Run: `cd /Users/zhoumo/Documents/Claude/collector && grep -n "installerHooks\|installer-hooks" src-tauri/tauri.conf.json`
Expected: `bundle.windows.nsis.installerHooks` 仍指向 `installer-hooks.nsi`（本次不改 tauri.conf，保留引用）。

- [ ] **Step 3: NSIS 语法自检（本机无法端到端跑，做静态检查）**

Run: `cd /Users/zhoumo/Documents/Claude/collector && grep -c "ffmpeg_yes\|ffmpeg_no\|ffmpeg_done\|NSIS_HOOK_POSTINSTALL\|!macroend" src-tauri/installer-hooks.nsi`
Expected: 标签与宏结构完整（`!macro`/`!macroend` 配对、三个 label 存在）。人工核对：`MessageBox` 的 IDYES/IDNO 目标标签存在、`Goto ffmpeg_done` 跳过 no 分支、`${If}`/`${EndIf}` 配对。

说明：NSIS 编译只在 Windows `tauri build` 时发生，macOS 本机无法验证编译。此步为逻辑/结构自检；实际运行验证在 Windows 打包环境完成（已在 spec 验证局限中声明）。

- [ ] **Step 4: 提交**

```bash
git add src-tauri/installer-hooks.nsi
git commit -m "feat(installer): Windows 安装时经确认按 x64/arm64 适配下载 ffmpeg"
```

---

## Task 4: 清理本地中间产物 + 整体验证

**Files:** 无（本地清理 + 验证）

- [ ] **Step 1: 清 /tmp 图标临时文件与旧备份**

Run: `rm -f /tmp/icon_src_512.png /tmp/icon_src_1024.png /tmp/icon_preview.png && rm -rf /tmp/icons_backup_* && echo "已清 /tmp 图标中间文件"`
Expected: 输出确认。（这些不在 git 内，纯本地清理。）

- [ ] **Step 2: 清 release 打包产物**

Run: `rm -rf /Users/zhoumo/Documents/Claude/collector/src-tauri/target/release/bundle && echo "已清 release bundle"`
Expected: 输出确认。（打包产物，下次 build 重生，不进 git。）

- [ ] **Step 3: 确认仓库不受清理影响**

Run: `cd /Users/zhoumo/Documents/Claude/collector && git status --short && echo "(空=仓库干净，清理未触碰版本控制内容)"`
Expected: 空（清理的都是未跟踪/忽略的本地文件）。

- [ ] **Step 4: 整体构建验证**

Run: `cd /Users/zhoumo/Documents/Claude/collector && npx tsc --noEmit && (cd src-tauri && cargo build 2>&1 | tail -3 && cargo test --lib 2>&1 | tail -6)`
Expected: tsc 无错误；cargo build 完成；cargo test --lib 全绿。

- [ ] **Step 5: 手动验证清单**

- 有 ffmpeg 时：视频正常播放（回归）。
- 无 ffmpeg 时（可临时改 PATH 或重命名系统 ffmpeg 验证）：播放显示多行详细提示（问题定位 + 各平台安装命令 + BtbN 链接），换行正常可读。
- Windows 安装器下载逻辑：留待 Windows `tauri build` 环境验证（本机不可测）。

- [ ] **Step 6: 前序任务已各自提交，本任务无额外提交**

---

## 自审记录

- **Spec 覆盖**：目标1（检测+提示）→ Task1（后端）+ Task2（前端展示）；目标2（Windows 可选下载）→ Task3（NSIS 升级）；目标3（清理）→ Task4。全覆盖。
- **验证局限已声明**：NSIS 部分本机无法端到端跑，计划中明确为静态/逻辑自检 + 留待 Windows 打包验证——不假装能验证。
- **保留项**：`ffmpeg_bin` 的 bin/ 查找不动（下载的 ffmpeg 放 bin/ 正好被它找到）；tauri.conf 的 nsis 引用保留（Task3 Step2 确认）。
- **无占位符**：每步含完整代码/命令与预期。NSIS 下载 URL 用 BtbN latest 稳定链接 + `$2` 架构变量，无硬编码版本号。
- **架构适配**：`$PROCESSOR_ARCHITECTURE` + `PROCESSOR_ARCHITEW6432` 双重判断 arm64，兜底 x64，匹配 BtbN 的 win64/winarm64 包。
