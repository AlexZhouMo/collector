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
