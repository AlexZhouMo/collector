; NSIS 安装钩子：检测 ffmpeg，未装则下载 essentials 到 $INSTDIR\bin
; 由 tauri.conf.json 的 bundle.windows.nsis.installerHooks 引用。
; 用 PowerShell 下载/解压，不依赖第三方 NSIS 插件（Tauri 自带的 NSIS 无 inetc）。

!macro NSIS_HOOK_POSTINSTALL
  ; 检测系统 PATH 里是否已有 ffmpeg
  nsExec::ExecToStack 'cmd /c ffmpeg -version'
  Pop $0 ; 退出码（0=已装）
  ${If} $0 != 0
    DetailPrint "未检测到 ffmpeg，正在下载运行时依赖（可能需要几分钟）..."
    CreateDirectory "$INSTDIR\bin"
    ; 用 PowerShell 下载 BtbN 静态构建 essentials（Windows 自带 powershell，无需插件）
    nsExec::ExecToLog 'powershell -NoProfile -Command "$ProgressPreference=''SilentlyContinue''; try { Invoke-WebRequest -Uri ''https://github.com/BtbN/FFmpeg-Builds/releases/download/latest/ffmpeg-master-latest-win64-gpl.zip'' -OutFile ''$INSTDIR\ffmpeg.zip'' } catch { exit 1 }"'
    Pop $1 ; 下载退出码
    ${If} $1 == 0
      nsExec::ExecToLog 'powershell -NoProfile -Command "Expand-Archive -Force ''$INSTDIR\ffmpeg.zip'' ''$INSTDIR\ffmpeg_tmp''"'
      nsExec::ExecToLog 'powershell -NoProfile -Command "Get-ChildItem -Recurse ''$INSTDIR\ffmpeg_tmp'' -Include ffmpeg.exe,ffprobe.exe | ForEach-Object { Copy-Item $_.FullName ''$INSTDIR\bin'' -Force }"'
      Delete "$INSTDIR\ffmpeg.zip"
      RMDir /r "$INSTDIR\ffmpeg_tmp"
      DetailPrint "ffmpeg 已安装到 $INSTDIR\bin"
    ${Else}
      DetailPrint "ffmpeg 下载失败，视频转码功能将不可用，可稍后手动安装 ffmpeg 或将其加入 PATH。"
    ${EndIf}
  ${EndIf}
!macroend
