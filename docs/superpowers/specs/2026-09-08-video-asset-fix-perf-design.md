# 视频播放：修复 asset 访问 + 打开/播放/拖进度性能极致

- 日期：2026-09-08
- 状态：设计待复审

## Context

视频改用「remux 成 mp4 + asset:// 用 `<video>` 播放」后，remux 产物正确（H264 视频 copy + 音频转 AAC），但前端报 **video error code 4「源不可用或格式不支持」**。dev 日志确证：`<video src="asset://localhost/%2Fvar%2Ffolders%2F...T%2Fcollector_xxx.mp4">` 加载失败。

**根因**：remux 产物放在 macOS **系统临时目录 `/var/folders/.../T/`**，Tauri asset 协议不放行该路径（尽管 assetProtocol scope 配了 `["**"]`）。对比证据：同样用 `convertFileSrc` + asset://，**封面图能加载**（PosterGrid/TreeView/GameView），因为封面在**素材根目录**（如 `/Users/zhoumo/Downloads/...`），asset 能访问；临时目录则被拒。

**用户已统一后续视频格式**：H264 + AC3 + 无字幕 + 720p + ~3G。据此把打开/播放/拖进度性能优化到极致。

## 已确认的设计决策

1. **产物改放应用数据目录**（app_data_dir，asset 已验证可访问）而非系统临时目录 → 修 code 4。
2. **打开性能：remux 缓存复用**。首次打开某视频 remux（视频 `-c:v copy` + 音频 `-c:a aac`，实测几秒），产物缓存到应用缓存目录；同一视频再开直接用缓存，秒开。不预转全部（不占双倍盘）。
3. **缓存清理：LRU 上限自动清**。缓存目录设上限（默认 20GB），超限删最久未使用的产物。

## 架构

在现有 remux 方案（`player/transcode.rs` + `player/mod.rs` + PlayerView）上做三处针对性改动，不引入新子系统。

### 后端 `src-tauri/src/player/transcode.rs`
- **缓存目录**：新增 `cache_dir(app_data: &Path) -> PathBuf`，返回 `<app_data>/video_cache/`（asset 可访问；启动时确保存在）。`temp_mp4_path` 改为 `cache_dir().join("collector_<hash>.mp4")`，不再用 `std::env::temp_dir()`。
- **remux** 签名加 `app_data: &Path` 参数（或让调用方传缓存目录）。复用逻辑保留（产物存在且非空则直接返回）。
- **remux 参数保持**：`-c:v copy -c:a aac -b:a 192k -movflags +faststart`（faststart 把 moov 移到头部→asset seek 快）。
- **LRU 清理**：新增 `enforce_cache_limit(cache_dir, max_bytes)`：列出缓存目录所有 `collector_*.mp4`，按 atime/mtime（用 mtime，remux 完成或复用时 touch 更新）排序，累计超 `max_bytes`（默认 20GB=20*1024³）则从最旧删起。在 remux 成功后调用一次。
- 单测：cache_dir 路径构造、LRU 选择删除逻辑（用临时目录造几个假文件测按 mtime 删最旧、保留在限内的）。

### 后端 `src-tauri/src/player/mod.rs` + `lib.rs`
- `player_open` 需要拿到 app_data_dir 传给 remux。方式：command 签名加 `app: tauri::AppHandle`，内部 `app.path().app_data_dir()` 取目录传给 remux。（PlayerState 存当前产物路径不变，但**不再在 player_stop 删产物**——改为缓存保留，由 LRU 管理。player_stop 只清 PlayerState 状态，不删文件。）
- 退出不删缓存（复用的前提）；LRU 在 remux 时统一管理磁盘。

### 前端 `src/views/PlayerView.ts`（基本不变，微调）
- 已有 loading 状态（"准备中…转封装" → "加载中…" → 播放）+ video error 诊断，保留。
- convertFileSrc(app_data 里的产物) → asset:// 应能访问，画面出来。
- 播放/拖进度：asset:// 播本地 mp4 是原生行为，seek 直接 `video.currentTime=`（faststart moov 在头部，seek 快）。已实现，无需改。

## 性能分析（为何这样已是极致）
- **打开**：视频 H264 免重编码（`-c:v copy`），只音频 AC3→AAC（3G 视频的音频轨很小，秒级）；缓存后重开瞬开。
- **播放**：`<video>` 硬件解码 H264 720p，原生流畅。
- **拖进度**：本地 mp4 + moov 头部（faststart）→ `<video>` 原生 seek，无重转、无延迟。
这三者用「一次 remux 缓存 + 本地 mp4 asset 播放」已达桌面端最优，无需自定义流/实时转码。

## 关键文件
- `src-tauri/src/player/transcode.rs`（缓存目录、LRU、remux 加 app_data 参数）
- `src-tauri/src/player/mod.rs`（player_open 取 app_data；player_stop 不删缓存）
- `src-tauri/src/lib.rs`（player_open 传 AppHandle——若签名变）
- `src/views/PlayerView.ts`（基本不变）

## 明确不做（YAGNI）
- 不预转码全部视频（不占双倍盘）
- 不支持非 H264 视频/字幕（用户格式已统一；非 H264 remux 失败则提示）
- 不做自定义流协议/实时转码（已证否）
- 缓存 LRU 上限暂硬编码 20GB（后续可设置页可配）

## 验证
1. 单测：cache_dir 路径、LRU 按 mtime 删最旧且保留限内文件。
2. 真机：点视频 → 几秒 remux（产物落 app_data/video_cache）→ `<video>` 经 asset:// **出画面 + 有声音**（修好 code 4）。
3. 同一视频再开 → 秒开（缓存复用）。
4. 拖进度：丝滑、无重新加载延迟。
5. 播放多个视频超 20GB → 最旧缓存被自动删。
6. 退出：立即切回、不崩、缓存保留。

## 风险
- app_data_dir 能否被 asset 访问：把握高（封面图已证素材目录可访问；app_data 是应用自有目录，asset 默认放行域）。若仍 code 4，退查 assetProtocol scope 是否需显式加 app_data 路径 / capabilities 权限。这是本方案唯一需真机确认的关键点。
