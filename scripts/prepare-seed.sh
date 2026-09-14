#!/usr/bin/env bash
# 把当前机器的应用数据快照复制为打包 seed（排除 video_cache 缓存）。
# 本地 macOS 运行一次，产物 src-tauri/resources/seed/ 提交进仓库供 CI 打包。
set -euo pipefail
SRC="$HOME/Library/Application Support/com.zhoumo.collector"
DST="$(cd "$(dirname "$0")/.." && pwd)/src-tauri/resources/seed"
rm -rf "$DST"
mkdir -p "$DST"
cp "$SRC/collector.sqlite" "$DST/collector.sqlite"
cp -R "$SRC/covers" "$DST/covers"
cp -R "$SRC/subtitles" "$DST/subtitles"
# 不复制 video_cache（缓存，数 GB）
echo "seed 生成于 $DST"
du -sh "$DST"
