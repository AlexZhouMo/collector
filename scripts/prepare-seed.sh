#!/usr/bin/env bash
# 把本机的应用数据快照同步为打包 seed，用于：
#   1) 打包 Windows/macOS 安装包时随包分发（Tauri bundle.resources），首启释放到用户 app_data
#   2) 其他机器 clone 源码跑 dev 时，首启同样自动释放，得到一致的初始数据
# 数据源：本机 macOS 的 ~/Library/Application Support/com.zhoumo.collector/
# 产物：src-tauri/resources/seed/（含 collector.sqlite + covers/ + subtitles/），入库
#
# 安全措施：
#   - 检测应用是否在跑，是则拒绝执行（避免 DB 锁定与写入不完整）
#   - 清空 seed DB 的 settings 表（防绝对路径与 TMDB API key 入库）
#   - 用 rsync --delete 覆盖，保证孤儿文件被清理，seed 与本机 bit-for-bit 一致
#   - 不复制 video_cache（缓存目录，可能数 GB）
#
# 用法：
#   bash scripts/prepare-seed.sh       # 交互式：显示统计后询问 y/n
#   bash scripts/prepare-seed.sh -y    # 无交互（CI 或脚本调用）
set -euo pipefail

SRC="$HOME/Library/Application Support/com.zhoumo.collector"
DST="$(cd "$(dirname "$0")/.." && pwd)/src-tauri/resources/seed"

# --- 前置检查 ---
if [[ ! -f "$SRC/collector.sqlite" ]]; then
  echo "错误：本机 app_data 里没有 collector.sqlite" >&2
  echo "路径：$SRC/collector.sqlite" >&2
  exit 1
fi

if pgrep -f "target/debug/collector|target/release/collector|Collector.app" >/dev/null 2>&1; then
  echo "错误：collector 应用正在运行（会锁 DB，导致 seed 不一致）" >&2
  echo "请关闭应用后重试。" >&2
  exit 1
fi

if ! command -v rsync >/dev/null 2>&1 || ! command -v sqlite3 >/dev/null 2>&1; then
  echo "错误：需要 rsync 与 sqlite3 命令" >&2
  exit 1
fi

# --- 交互确认（-y 跳过）---
NOCONFIRM=false
[[ "${1:-}" == "-y" ]] && NOCONFIRM=true

SRC_SIZE=$(du -sh "$SRC/collector.sqlite" "$SRC/covers" "$SRC/subtitles" 2>/dev/null | tail -1 | awk '{print $1}' || echo "?")
SRC_MEDIA=$(sqlite3 "$SRC/collector.sqlite" "SELECT count(*) FROM media" 2>/dev/null || echo "?")
SRC_COMIC=$(sqlite3 "$SRC/collector.sqlite" "SELECT count(*) FROM comic" 2>/dev/null || echo "?")
SRC_GAME=$(sqlite3 "$SRC/collector.sqlite" "SELECT count(*) FROM game" 2>/dev/null || echo "?")

echo "=== 本机数据快照 ==="
echo "  DB: media=$SRC_MEDIA / comic=$SRC_COMIC / game=$SRC_GAME"
echo "  路径: $SRC"
echo ""
echo "=== 同步目标 ==="
echo "  $DST"
echo ""

if ! $NOCONFIRM; then
  read -r -p "确认将本机数据完整同步到 seed（覆盖旧 seed）？[y/N] " ans
  [[ "$ans" == "y" || "$ans" == "Y" ]] || { echo "已取消"; exit 0; }
fi

# --- 执行同步 ---
mkdir -p "$DST"

echo ""
echo "[1/4] 覆盖 DB..."
cp "$SRC/collector.sqlite" "$DST/collector.sqlite"

echo "[2/4] 同步 covers (rsync --delete)..."
rsync -a --delete "$SRC/covers/" "$DST/covers/"

echo "[3/4] 同步 subtitles (rsync --delete)..."
rsync -a --delete "$SRC/subtitles/" "$DST/subtitles/"

echo "[4/4] 清空 seed DB 的 settings 表（防绝对路径与 TMDB API key 入库）..."
sqlite3 "$DST/collector.sqlite" "DELETE FROM settings; VACUUM;"

# --- 结果报告 ---
DST_SIZE=$(du -sh "$DST" | awk '{print $1}')
DST_MEDIA=$(sqlite3 "$DST/collector.sqlite" "SELECT count(*) FROM media")
DST_COMIC=$(sqlite3 "$DST/collector.sqlite" "SELECT count(*) FROM comic")
DST_GAME=$(sqlite3 "$DST/collector.sqlite" "SELECT count(*) FROM game")
DST_SETTINGS=$(sqlite3 "$DST/collector.sqlite" "SELECT count(*) FROM settings")

echo ""
echo "=== 完成 ==="
echo "  seed 大小: $DST_SIZE"
echo "  seed DB: media=$DST_MEDIA / comic=$DST_COMIC / game=$DST_GAME"
echo "  settings 表条数: $DST_SETTINGS (应为 0，防泄漏)"
echo ""
echo "下一步：git add src-tauri/resources/seed/ && git commit -m 'chore(seed): 同步本机数据' && git push"
