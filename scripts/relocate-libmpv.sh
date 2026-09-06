#!/usr/bin/env bash
# 打包后重定位 libmpv 引用，使 Collector.app 可分发到未安装 mpv 的 macOS 机器。
#
# 背景：libmpv.2.dylib 的 install_name 是绝对路径
#   /opt/homebrew/opt/mpv/lib/libmpv.2.dylib
# 因此 tauri build 产出的可执行文件 load command 记录的也是该绝对路径，
# 即使已注入 rpath（@executable_path/../Frameworks，见 build.rs）也不会生效。
# 本脚本用 install_name_tool 把引用改为 @rpath/libmpv.2.dylib。
#
# 用法（在 `npm run tauri build` 产出 .app 之后运行）：
#   ./scripts/relocate-libmpv.sh [路径/Collector.app]
# 不传参数则默认使用 release 产物路径。
#
# 注意：install_name_tool 会使已有代码签名失效。正式分发时需在本步骤之后
# 重新签名并公证：
#   codesign --force --deep --sign "Developer ID Application: ..." Collector.app
#
# CI 建议：作为构建后钩子执行本脚本，随后重签名 / 公证 / 打 DMG。
set -euo pipefail

APP="${1:-src-tauri/target/release/bundle/macos/Collector.app}"
EXE="$APP/Contents/MacOS/Collector"
DYLIB="$APP/Contents/Frameworks/libmpv.2.dylib"
OLD_REF="/opt/homebrew/opt/mpv/lib/libmpv.2.dylib"   # Intel: /usr/local/opt/mpv/lib/libmpv.2.dylib

if [[ ! -f "$EXE" ]]; then echo "找不到可执行文件: $EXE" >&2; exit 1; fi
if [[ ! -f "$DYLIB" ]]; then echo "找不到打包的 dylib: $DYLIB" >&2; exit 1; fi

echo "重写 dylib install_name -> @rpath/libmpv.2.dylib"
install_name_tool -id @rpath/libmpv.2.dylib "$DYLIB"

echo "重写可执行文件引用 -> @rpath/libmpv.2.dylib"
install_name_tool -change "$OLD_REF" @rpath/libmpv.2.dylib "$EXE"

echo "完成。校验："
otool -L "$EXE" | grep -i mpv || true
echo "提醒：install_name_tool 已使代码签名失效，正式分发请重新签名 / 公证。"
