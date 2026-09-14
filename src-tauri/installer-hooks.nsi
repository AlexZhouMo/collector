; NSIS 安装钩子：检测 ffmpeg，未装则下载 essentials 到 $INSTDIR\bin
; 由 tauri.conf.json 的 bundle.windows.nsis.installerHooks 引用。
; 依赖：inetc 下载插件（Tauri 打包的 NSIS 通常自带）。若 CI 报缺插件，
; 可改用 powershell Invoke-WebRequest 或 NSISdl::download 下载。

!macro NSIS_HOOK_POSTINSTALL
  ; 检测系统 PATH 里是否已有 ffmpeg
  nsExec::ExecToStack 'cmd /c ffmpeg -version'
  Pop $0 ; 退出码（0=已装）
  ${If} $0 != 0
    DetailPrint "未检测到 ffmpeg，正在下载运行时依赖..."
    CreateDirectory "$INSTDIR\bin"
    inetc::get /caption "下载 ffmpeg" /cancel \
      "https://github.com/BtbN/FFmpeg-Builds/releases/download/latest/ffmpeg-master-latest-win64-gpl.zip" \
      "$INSTDIR\ffmpeg.zip" /end
    Pop $1
    ${If} $1 == "OK"
      nsExec::ExecToLog 'powershell -NoProfile -Command "Expand-Archive -Force ''$INSTDIR\ffmpeg.zip'' ''$INSTDIR\ffmpeg_tmp''"'
      nsExec::ExecToLog 'powershell -NoProfile -Command "Get-ChildItem -Recurse ''$INSTDIR\ffmpeg_tmp'' -Include ffmpeg.exe,ffprobe.exe | ForEach-Object { Copy-Item $_.FullName ''$INSTDIR\bin'' -Force }"'
      Delete "$INSTDIR\ffmpeg.zip"
      RMDir /r "$INSTDIR\ffmpeg_tmp"
      DetailPrint "ffmpeg 已安装到 $INSTDIR\bin"
    ${Else}
      DetailPrint "ffmpeg 下载失败（$1），视频转码功能将不可用，可稍后手动安装 ffmpeg 或将其加入 PATH。"
    ${EndIf}
  ${EndIf}
!macroend
