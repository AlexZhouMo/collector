//! 视频转码：把 H.264 MKV 用 ffmpeg 转成 HLS（一份 `.m3u8` playlist + 若干 `.ts` segment
//! 放缓存目录），前端用 hls.js 走 MSE 播放。
//! - 视频：始终 `-c:v copy`，无损转封装。
//! - 音频：AC-3 → AAC（Chromium 无 AC-3 解码器），其他编码保持 `-c:a copy`。
//! - 封装：HLS event-list（`-f hls -hls_time 2 -hls_playlist_type event`），
//!   段边转边写，hls.js 拿到首段（~2s）就能起播——**开播时间与总时长解耦**。
//!
//! 缓存复用 + LRU 上限清理（按 hash 前缀分组、组内 playlist + 全部 segment 一起删）。
//!
//! 缓存文件命名：
//! - `collector_HASH.m3u8`：完成态 playlist（末尾含 `#EXT-X-ENDLIST`）
//! - `collector_HASH.m3u8.part`：转码进行中的 playlist（成功后 rename 为最终名）
//! - `collector_HASH_%05d.ts`：segments（用 `-hls_flags temp_file` 保证 rename 前不被读到半段）
//!
//! 决定"缓存是否完整可复用"只看最终 `.m3u8` 是否存在。
//!
//! 历史：曾试过单文件 MP4（moov-tail）与 fragmented MP4，均无法在转码完成前起播。
//! fMP4 甚至因 Chromium `<video>` 内建 demuxer 需扫遍全部 moof 建索引，3.48GB 文件
//! canplay 要 46 秒。HLS 是本地转码+浏览器播放的行业标准（Plex/Jellyfin 同款）。
use crate::error::{AppError, AppResult};
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};

/// 缓存上限（字节）。默认 20 GiB。
pub const CACHE_LIMIT_BYTES: u64 = 20 * 1024 * 1024 * 1024;

/// 转码策略版本。改变音频处理策略或封装格式时递增，让老缓存的产物因文件名哈希改变
/// 而自然作废，LRU 逐步淘汰，无需手动清缓存。
/// v1: 视频/音频均 `-c copy`，普通 MP4 moov-tail（Windows 上 AC-3 无声）
/// v2: 视频 copy；AC-3→AAC，其他音频 copy；仍普通 MP4 moov-tail
/// v3/v4: fragmented MP4 尝试——canplay 慢 40+ 倍，废弃
/// v5: 回到 v2 的普通 MP4 moov-tail；转码完成才起播（打开=转码时长）
/// v6: **改用 HLS**：`.m3u8` + `.ts` segments，hls.js/MSE 播放；首段就绪即起播
/// v7: HLS + 续转支持——ffmpeg 直接写 `.m3u8`（无 rename 中间态，避免 asset 协议按精确名 404）；
///     用户按返回时 ffmpeg 走 `q\n` 优雅退出（写完当前段 + 更新 playlist）；下次打开
///     同视频走续转分支（`-ss` + `append_list` 追加到已有 playlist）；转码完整（有
///     `#EXT-X-ENDLIST`）才算 Ready 秒开
const TRANSCODE_STRATEGY_VERSION: u32 = 7;

/// 定位 ffmpeg：优先用启动时解析的内置 sidecar 绝对路径；
/// 开发态/未初始化时回退裸命令名走 PATH（便于 tauri dev / cargo test）。
fn ffmpeg_bin() -> PathBuf {
    crate::player::ffmpeg_paths::ffmpeg().unwrap_or_else(|| PathBuf::from("ffmpeg"))
}

/// 构造 ffmpeg 子进程命令。Windows 上加 `CREATE_NO_WINDOW`：
/// 主程序是 GUI 子系统（无控制台），不加此标志时每个控制台子进程都会弹出一个
/// 命令行窗口——用户关掉它还会让 ffmpeg 提前收尾并写下 `#EXT-X-ENDLIST`，留下
/// "看似完整、实则截断"的缓存。
fn ffmpeg_command() -> Command {
    #[allow(unused_mut)]
    let mut cmd = Command::new(ffmpeg_bin());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    cmd
}

/// 检测 ffmpeg 是否可用（能 spawn 且 -version 成功）。
/// 缺失时返回含详细引导的 Err，供前端展示。成功结果在进程内缓存——
/// 不必每次打开视频都多起一个子进程。
fn ensure_ffmpeg_available() -> AppResult<()> {
    use std::sync::atomic::{AtomicBool, Ordering};
    static VERIFIED: AtomicBool = AtomicBool::new(false);
    if VERIFIED.load(Ordering::Relaxed) {
        return Ok(());
    }
    let ok = ffmpeg_command()
        .arg("-version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);
    if !ok {
        return Err(AppError::Other(ffmpeg_missing_hint()));
    }
    VERIFIED.store(true, Ordering::Relaxed);
    Ok(())
}

/// 内置 ffmpeg 缺失或损坏时的中文提示（引导重装）。
fn ffmpeg_missing_hint() -> String {
    "视频播放需要内置的 ffmpeg 组件，但未找到或无法运行。\n\n\
     Collector 已随安装包内置 ffmpeg，此提示通常意味着安装文件损坏或被安全软件拦截。\n\n\
     【解决办法】请重新下载并安装 Collector；若仍失败，可将应用加入安全软件白名单后重试。"
        .to_string()
}

/// 源文件里的一条音轨（顺序即 ffmpeg 的 `0:a:N`）。
#[derive(Debug, Clone, PartialEq, Default)]
pub struct AudioTrack {
    /// 编码名，小写：`ac3` / `aac` / `dts` / `truehd` …
    pub codec: String,
    /// 语言代码（`eng` / `chi` …），源文件没标则 None。
    pub language: Option<String>,
    /// 音轨标题（Matroska 的 title 标签），没有则 None。
    pub title: Option<String>,
    /// 声道布局原文：`stereo` / `5.1(side)` / `mono` …
    pub channels: Option<String>,
    /// 源文件标记的默认音轨。
    pub default: bool,
}

/// `probe_meta` 的结果：时长 + 全部音轨。
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ProbeMeta {
    /// 秒；探测不到为 0。
    pub duration: f64,
    pub audio: Vec<AudioTrack>,
}

impl ProbeMeta {
    /// 第一条音轨的编码（单音轨转码分支据此决定是否转 AAC）。
    pub fn first_audio_codec(&self) -> Option<&str> {
        self.audio.first().map(|a| a.codec.as_str())
    }
}

/// 用 ffmpeg 一次取视频时长与全部音轨信息。
///
/// 只给输入不给输出时，ffmpeg 打印完输入信息就以非 0 退出（"At least one output file
/// must be specified"）——这是预期行为，信息都在 stderr 里。借此省掉单独的 ffprobe
/// 二进制（约 80–94 MB）。
pub fn probe_meta(path: &str) -> AppResult<ProbeMeta> {
    let out = ffmpeg_command()
        .args(["-hide_banner", "-nostdin", "-i", path])
        .output()
        .map_err(|e| AppError::Other(format!("ffmpeg spawn: {e}")))?;
    let text = String::from_utf8_lossy(&out.stderr);
    match parse_probe_output(&text) {
        Some(meta) => Ok(meta),
        None => Err(AppError::Other(format!(
            "无法读取视频信息：{}",
            text.lines().rfind(|l| !l.trim().is_empty()).unwrap_or("").trim()
        ))),
    }
}

/// 解析 `ffmpeg -i <file>` 的 stderr：
/// - `  Duration: 01:46:05.41, start: ...` → 秒（`N/A` → 0.0）
/// - 每条 `Stream #0:1(eng): Audio: ac3, 48000 Hz, 5.1(side), fltp, 384 kb/s (default)`
///   → 一条音轨（编码小写、语言、声道布局、是否默认）；其后 `Metadata:` 里的 `title` 行
///   归到这条音轨上。
///
/// 没有 `Duration:` 行说明输入没打开（文件损坏/格式不认），返回 None。
fn parse_probe_output(text: &str) -> Option<ProbeMeta> {
    let mut duration: Option<f64> = None;
    let mut audio: Vec<AudioTrack> = Vec::new();
    // 当前正在读哪条音轨的附属行（Metadata）。遇到任何别的 Stream 行就清掉，
    // 免得把视频/字幕流的 title 记到音轨头上。
    let mut in_audio = false;
    for line in text.lines() {
        let line = line.trim();
        if duration.is_none() {
            if let Some(rest) = line.strip_prefix("Duration: ") {
                let ts = rest.split(',').next().unwrap_or("").trim();
                duration = Some(parse_hms(ts).unwrap_or(0.0));
                continue;
            }
        }
        if line.starts_with("Stream #") {
            in_audio = false;
            if let Some((head, rest)) = line.split_once(": Audio: ") {
                if let Some(track) = parse_audio_stream(head, rest) {
                    audio.push(track);
                    in_audio = true;
                }
            }
            continue;
        }
        if in_audio {
            if let Some((key, value)) = line.split_once(':') {
                let value = value.trim();
                if key.trim().eq_ignore_ascii_case("title") && !value.is_empty() {
                    if let Some(track) = audio.last_mut() {
                        track.title.get_or_insert_with(|| value.to_string());
                    }
                }
            }
        }
    }
    duration.map(|duration| ProbeMeta { duration, audio })
}

/// 解析一条音频流行。`head` 是 `: Audio: ` 之前的部分（`Stream #0:1(eng)`），
/// `rest` 是之后的部分（`ac3, 48000 Hz, 5.1(side), fltp, 384 kb/s (default)`）。
fn parse_audio_stream(head: &str, rest: &str) -> Option<AudioTrack> {
    let codec: String = rest
        .chars()
        .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
        .collect();
    if codec.is_empty() {
        return None;
    }
    // 语言在流编号后的圆括号里：`Stream #0:1(eng)`、`Stream #0:1[0x1100](chi)`
    let language = head
        .strip_suffix(')')
        .and_then(|h| h.rsplit_once('('))
        .map(|(_, lang)| lang.trim().to_ascii_lowercase())
        .filter(|l| !l.is_empty() && l != "und");
    // 声道布局紧跟在采样率（`48000 Hz`）后面
    let parts: Vec<&str> = rest.split(", ").collect();
    let channels = parts
        .iter()
        .position(|p| p.trim_end().ends_with(" Hz"))
        .and_then(|i| parts.get(i + 1))
        .map(|c| c.trim().trim_end_matches(" (default)").trim().to_string())
        .filter(|c| !c.is_empty());
    Some(AudioTrack {
        codec: codec.to_ascii_lowercase(),
        language,
        title: None,
        channels,
        default: rest.contains("(default)"),
    })
}

/// 语言代码 → 中文名（界面上的音轨标签用）。不认识的返回 None，标签里就不写语言。
fn language_name(code: &str) -> Option<&'static str> {
    Some(match code {
        "chi" | "zho" | "zh" => "中文",
        "cmn" => "国语",
        "yue" => "粤语",
        "eng" | "en" => "英语",
        "jpn" | "ja" => "日语",
        "kor" | "ko" => "韩语",
        "fre" | "fra" | "fr" => "法语",
        "ger" | "deu" | "de" => "德语",
        "spa" | "es" => "西班牙语",
        "ita" | "it" => "意大利语",
        "rus" | "ru" => "俄语",
        "por" | "pt" => "葡萄牙语",
        "tha" | "th" => "泰语",
        "hin" | "hi" => "印地语",
        _ => return None,
    })
}

/// 编码名的展示写法。
fn codec_display(codec: &str) -> String {
    match codec {
        "ac3" => "AC3".into(),
        "eac3" => "E-AC3".into(),
        "dts" => "DTS".into(),
        "truehd" => "TrueHD".into(),
        "aac" => "AAC".into(),
        "mp3" => "MP3".into(),
        "flac" => "FLAC".into(),
        "opus" => "Opus".into(),
        "vorbis" => "Vorbis".into(),
        other => other.to_ascii_uppercase(),
    }
}

/// 声道布局的展示写法：`stereo` → `2.0`，`5.1(side)` → `5.1`，`mono` → `1.0`。
fn channels_display(layout: &str) -> String {
    let base = layout.split('(').next().unwrap_or(layout).trim();
    match base {
        "stereo" => "2.0".into(),
        "mono" => "1.0".into(),
        other => other.to_string(),
    }
}

/// 标题是否已经点明了这门语言（不区分大小写）。`lang_name` 是 `language_name` 给出的中文名。
fn title_names_language(title: &str, lang_name: &str) -> bool {
    let aliases: &[&str] = match lang_name {
        "中文" | "国语" | "粤语" => &["中文", "国语", "粤语", "普通话", "汉语", "国配", "台配", "mandarin", "cantonese", "chinese"],
        "英语" => &["英语", "英文", "english"],
        "日语" => &["日语", "日文", "japanese"],
        "韩语" => &["韩语", "韩文", "korean"],
        _ => &[],
    };
    let lower = title.to_lowercase();
    lower.contains(&lang_name.to_lowercase()) || aliases.iter().any(|a| lower.contains(a))
}

/// 音轨在界面上的标签，如 `英语 · DTS-HD MA (DTS 5.1)`、`中文 (AAC 2.0)`、`音轨 3 (DTS 5.1)`。
/// `number` 是从 1 起的序号，只在既没有语言也没有标题时用到。
fn audio_label(track: &AudioTrack, number: usize) -> String {
    let lang = track.language.as_deref().and_then(language_name);
    let title = track.title.as_deref().map(str::trim).filter(|t| !t.is_empty());
    let main = match (lang, title) {
        // 标题里已经点明了语言（`国语`、`English 5.1`）就不再重复写一遍
        (Some(l), Some(t)) if title_names_language(t, l) => t.to_string(),
        (Some(l), Some(t)) => format!("{l} · {t}"),
        (Some(l), None) => l.to_string(),
        (None, Some(t)) => t.to_string(),
        (None, None) => format!("音轨 {number}"),
    };
    let detail = match track.channels.as_deref() {
        Some(ch) => format!("{} {}", codec_display(&track.codec), channels_display(ch)),
        None => codec_display(&track.codec),
    };
    format!("{main} ({detail})")
}

/// 全部音轨的标签，保证互不相同（HLS 要求同组内 NAME 唯一）：重名的在后面补 ` #序号`。
fn audio_labels(tracks: &[AudioTrack]) -> Vec<String> {
    let mut labels: Vec<String> = tracks
        .iter()
        .enumerate()
        .map(|(i, t)| audio_label(t, i + 1))
        .collect();
    let originals = labels.clone();
    for (i, label) in labels.iter_mut().enumerate() {
        if originals.iter().filter(|l| **l == originals[i]).count() > 1 {
            *label = format!("{label} #{}", i + 1);
        }
    }
    labels
}

/// 默认音轨的下标：源文件标了 default 的第一条；都没标就第一条。
fn default_audio_index(tracks: &[AudioTrack]) -> usize {
    tracks.iter().position(|t| t.default).unwrap_or(0)
}

/// `HH:MM:SS.ss` → 秒。
fn parse_hms(ts: &str) -> Option<f64> {
    let mut parts = ts.split(':');
    let h: f64 = parts.next()?.parse().ok()?;
    let m: f64 = parts.next()?.parse().ok()?;
    let s: f64 = parts.next()?.parse().ok()?;
    if parts.next().is_some() {
        return None;
    }
    Some(h * 3600.0 + m * 60.0 + s)
}

/// 缓存文件名前缀（`collector_HASH`）：策略版本 + 源路径的哈希。
/// 同一源视频的 playlist + 全部 segment 都用这个前缀，方便按组 LRU/清理。
pub fn cache_prefix(src: &str) -> String {
    let mut h = DefaultHasher::new();
    TRANSCODE_STRATEGY_VERSION.hash(&mut h);
    src.hash(&mut h);
    format!("collector_{:016x}", h.finish())
}

/// 缓存的 HLS playlist 最终路径：`cache_dir/collector_HASH.m3u8`。
pub fn cached_playlist_path(cache_dir: &Path, src: &str) -> PathBuf {
    cache_dir.join(format!("{}.m3u8", cache_prefix(src)))
}

/// 解析已有 playlist，返回**下一个应写入的段序号**。
/// - 未完成 playlist（无 ENDLIST）有 N 个 EXTINF 引用 `HASH_00000.ts` .. `HASH_(N-1).ts` 则返 N
/// - 空/坏/无匹配段 → None（视为无法续转，需从头开始）
/// - HLS event 模式段号严格递增无跳跃，取"最大段号 + 1"即可
pub fn next_segment_number(playlist_path: &Path, prefix: &str) -> Option<u32> {
    let content = std::fs::read_to_string(playlist_path).ok()?;
    let seg_prefix = format!("{prefix}_");
    let mut max_seg: Option<u32> = None;
    for line in content.lines() {
        // 段行形如 `collector_HASH_00042.ts`（basename，无路径）
        let line = line.trim();
        if let Some(rest) = line.strip_prefix(&seg_prefix) {
            if let Some(num_str) = rest.strip_suffix(".ts") {
                if let Ok(n) = num_str.parse::<u32>() {
                    max_seg = Some(max_seg.map_or(n, |m| m.max(n)));
                }
            }
        }
    }
    max_seg.map(|m| m + 1)
}

/// 判断 playlist 是否完整（包含 HLS 结束标记 `#EXT-X-ENDLIST`）。
/// 转码进行中的 `.m3u8` 存在但无此标记；成功完成的有；异常中止的可能是"存在但无标记"。
/// 只读文件末尾 1KB（足以覆盖 ENDLIST 附近的行），不 IO 整片。
pub fn playlist_is_complete(path: &Path) -> bool {
    use std::io::{Read, Seek, SeekFrom};
    let mut f = match std::fs::File::open(path) { Ok(f) => f, Err(_) => return false };
    let len = f.metadata().map(|m| m.len()).unwrap_or(0);
    if len == 0 { return false; }
    let read_len = len.min(1024);
    let start = len - read_len;
    if f.seek(SeekFrom::Start(start)).is_err() { return false; }
    let mut buf = vec![0u8; read_len as usize];
    if f.read_exact(&mut buf).is_err() { return false; }
    // ENDLIST 出现即完整
    buf.windows(15).any(|w| w == b"#EXT-X-ENDLIST\n")
        || buf.windows(14).any(|w| w == b"#EXT-X-ENDLIST")
}

/// playlist 里所有段的总时长（各 `#EXTINF:` 之和，秒）。读不到返回 0。
pub fn playlist_total_duration(path: &Path) -> f64 {
    let content = match std::fs::read_to_string(path) { Ok(c) => c, Err(_) => return 0.0 };
    content
        .lines()
        .filter_map(|l| l.trim().strip_prefix("#EXTINF:"))
        .filter_map(|v| v.split(',').next()?.trim().parse::<f64>().ok())
        .sum()
}

/// playlist 当前列出的段数（首段落盘 → 前端可以起播）。
/// ffmpeg 重写 playlist 的瞬间可能读不到/被拒绝访问，一律当作 0（"还没好"）。
pub fn playlist_segment_count(path: &Path) -> usize {
    std::fs::read_to_string(path)
        .map(|c| c.lines().filter(|l| l.trim_end().ends_with(".ts")).count())
        .unwrap_or(0)
}

/// "带 ENDLIST 但明显短于源时长"的 playlist 视为截断缓存（转码被外力提前收尾），不可复用。
/// 容差：源时长的 2% 与 10 秒取大者（容器时长与实际流时长本就有小出入）。
fn is_truncated(playlist_secs: f64, source_secs: f64) -> bool {
    source_secs > 0.0 && playlist_secs + (source_secs * 0.02).max(10.0) < source_secs
}

/// segments 命名模板：`cache_dir/collector_HASH_%05d.ts`，喂给 ffmpeg 的
/// `-hls_segment_filename`。segment URI 在 playlist 里是相对路径的 basename。
fn segment_pattern(cache_dir: &Path, prefix: &str) -> PathBuf {
    cache_dir.join(format!("{}_%05d.ts", prefix))
}

/// 从缓存文件名提取"组前缀"（`collector_HASH`）：
/// - `collector_HASH.m3u8` → `collector_HASH`
/// - `collector_HASH_NNNNN.ts` → `collector_HASH`
/// - 多音轨分流产物 `collector_HASH_v.m3u8` / `collector_HASH_a0.m3u8` /
///   `collector_HASH_v_NNNNN.ts` / `collector_HASH_a0_NNNNN.ts` → `collector_HASH`
/// - 其它（含无关文件）→ None
fn extract_cache_prefix(name: &str) -> Option<String> {
    if !name.starts_with("collector_") { return None; }
    if let Some(base) = name.strip_suffix(".m3u8") {
        return Some(strip_variant(base).to_string());
    }
    if let Some(base) = name.strip_suffix(".ts") {
        if let Some(idx) = base.rfind('_') {
            let (prefix, tail) = base.split_at(idx);
            let num = &tail[1..]; // 跳过下划线
            if !num.is_empty() && num.chars().all(|c| c.is_ascii_digit()) {
                return Some(strip_variant(prefix).to_string());
            }
        }
    }
    None
}

/// 去掉多音轨分流产物的流后缀：`collector_HASH_v`（视频）/ `collector_HASH_a0`（音轨）
/// → `collector_HASH`。单流产物没有后缀，原样返回。
fn strip_variant(base: &str) -> &str {
    if let Some((head, tail)) = base.rsplit_once('_') {
        let is_variant = tail == "v"
            || (tail.len() > 1
                && tail.starts_with('a')
                && tail[1..].chars().all(|c| c.is_ascii_digit()));
        // head 必须仍带着 `_HASH`：否则这段 `a123…` 其实就是哈希本身，不是音轨后缀
        if is_variant && head.len() > "collector_".len() && head.starts_with("collector_") {
            return head;
        }
    }
    base
}

/// 判断是否为**任何版本**的本模块缓存文件（含 v1-v5 `.mp4`、v6 `.m3u8.part` 等历史格式）。
/// 用于 `cache_used_bytes` / `clear_cache`——老升级用户的目录里可能残留这些，需一并统计/清理。
/// LRU 只管当前 v7 HLS 格式（`extract_cache_prefix`），历史格式不参与 mtime 淘汰、只靠"清空缓存"清。
fn is_any_cache_file(name: &str) -> bool {
    if !name.starts_with("collector_") { return false; }
    name.ends_with(".m3u8")
        || name.ends_with(".m3u8.part") // v6 中间态
        || name.ends_with(".ts")
        || name.ends_with(".ts.tmp")    // v7 段中间态
        || name.ends_with(".mp4")       // v1-v5 单文件
        || name.ends_with(".mp4.part")  // v1-v5 中间态
}

/// 统计缓存目录中所有本模块管理的文件（含历史格式）总字节数。
pub fn cache_used_bytes(cache_dir: &Path) -> u64 {
    let rd = match std::fs::read_dir(cache_dir) { Ok(r) => r, Err(_) => return 0 };
    rd.filter_map(|e| e.ok())
        .filter(|e| e.file_name().to_str().map(is_any_cache_file).unwrap_or(false))
        .filter_map(|e| e.metadata().ok())
        .map(|m| m.len())
        .sum()
}

/// 清空缓存目录中所有本模块管理的文件（含历史格式 v1-v5 `.mp4`、v6 `.m3u8.part` 等）。
/// 返回删除的文件数。只删本模块管理的文件，不碰目录中其他内容。
pub fn clear_cache(cache_dir: &Path) -> usize {
    let rd = match std::fs::read_dir(cache_dir) { Ok(r) => r, Err(_) => return 0 };
    let mut removed = 0;
    for e in rd.filter_map(|e| e.ok()) {
        if e.file_name().to_str().map(is_any_cache_file).unwrap_or(false)
            && std::fs::remove_file(e.path()).is_ok()
        {
            removed += 1;
        }
    }
    removed
}

/// 删除某组前缀（`collector_HASH`）的所有 .ts 段（不含 .m3u8/.m3u8.part）。
/// 用于转码失败/中止时清理残留段。
pub fn remove_segments_by_prefix(cache_dir: &Path, prefix: &str) {
    let rd = match std::fs::read_dir(cache_dir) { Ok(r) => r, Err(_) => return };
    for e in rd.filter_map(|e| e.ok()) {
        let name = match e.file_name().into_string() { Ok(n) => n, Err(_) => continue };
        if !name.starts_with(prefix) { continue; }
        if extract_cache_prefix(&name).as_deref() != Some(prefix) { continue; }
        if name.ends_with(".ts") {
            let _ = std::fs::remove_file(e.path());
        }
    }
}

/// 更新 mtime 为现在（复用时调用，让常看的视频在 LRU 中更新鲜）。
pub fn touch(p: &Path) {
    let _ = filetime::set_file_mtime(p, filetime::FileTime::now());
}

/// 缓存组：一个 hash 前缀对应的 playlist + 全部 segments。LRU 单元。
struct CacheGroup {
    /// 组内所有文件路径（playlist、可能的 .part、所有 .ts）
    files: Vec<PathBuf>,
    /// 组总字节数
    total: u64,
    /// LRU 用的时间戳：优先取 playlist 的 mtime；无 playlist 时取组内最新文件的 mtime。
    mtime: filetime::FileTime,
}

/// 扫描缓存目录，按 hash 前缀分组，返回每组的元数据。用于 LRU 淘汰。
fn enumerate_cache_groups(cache_dir: &Path) -> Vec<CacheGroup> {
    use std::collections::HashMap;
    let mut by_prefix: HashMap<String, CacheGroup> = HashMap::new();
    let rd = match std::fs::read_dir(cache_dir) { Ok(r) => r, Err(_) => return Vec::new() };
    for e in rd.filter_map(|e| e.ok()) {
        let name = match e.file_name().into_string() { Ok(n) => n, Err(_) => continue };
        let prefix = match extract_cache_prefix(&name) { Some(p) => p, None => continue };
        let meta = match e.metadata() { Ok(m) => m, Err(_) => continue };
        let mt = filetime::FileTime::from_last_modification_time(&meta);
        let is_playlist = name == format!("{prefix}.m3u8");
        let group = by_prefix.entry(prefix).or_insert_with(|| CacheGroup {
            files: Vec::new(),
            total: 0,
            mtime: filetime::FileTime::from_unix_time(0, 0),
        });
        group.files.push(e.path());
        group.total += meta.len();
        // playlist 优先决定组 mtime（touch 更新的就是 playlist）；否则取最新的一个文件
        if is_playlist || mt > group.mtime {
            group.mtime = mt;
        }
    }
    by_prefix.into_values().collect()
}

/// 强制缓存目录总大小 <= max_bytes：按组 mtime 从最旧开始整组删。
pub fn enforce_cache_limit(cache_dir: &Path, max_bytes: u64) {
    let mut groups = enumerate_cache_groups(cache_dir);
    let total: u64 = groups.iter().map(|g| g.total).sum();
    if total <= max_bytes { return; }
    groups.sort_by_key(|g| g.mtime); // 最旧在前
    let mut cur = total;
    for g in groups {
        if cur <= max_bytes { break; }
        for f in &g.files {
            let _ = std::fs::remove_file(f);
        }
        cur = cur.saturating_sub(g.total);
    }
}

/// `start_remux` 的结果：产物已完整（复用）或已后台启动转码。
pub enum TranscodeStatus {
    /// 完整 playlist 已存在，直接播放。附最终 playlist 路径与时长。
    Ready(PathBuf, f64),
    /// 已后台 spawn ffmpeg，边转边写 segments。附子进程 handle 与时长。
    Started(TranscodeHandle, f64),
}

/// 后台转码 handle：子进程 + 最终 playlist 路径 + 组前缀 + stdin 句柄。
/// 调用方负责保存 child（供 kill 或写 `q\n` 优雅退出）并读 stderr 判成功；
/// kill/失败时用 prefix 清 .ts 段与半吊 .m3u8。
pub struct TranscodeHandle {
    pub child: Child,
    /// 前端加载的入口 playlist：单流是 `collector_HASH.m3u8`（转码中直接写这里）；
    /// 多音轨分流是我们自己写的主 playlist（同名），它再引用视频与各音轨的 playlist。
    pub final_path: PathBuf,
    /// 用来量进度（已转时长）的 playlist：单流同 `final_path`；分流是视频那条。
    pub progress_path: PathBuf,
    /// ffmpeg 正在写的全部 playlist：单流只有一条；分流是视频 + 每条音轨。
    /// "首段就绪""转码完整"都要求它们全部满足。
    pub playlists: Vec<PathBuf>,
    pub cache_prefix: String,    // "collector_HASH"，供 kill/失败时清理
    /// 续转起点（秒）；从头转为 0。ffmpeg `-progress` 的 out_time 相对它计，算总进度时要加回。
    pub resume_offset: f64,
    /// 启动时 playlist 里已有的段数（续转）；从头转为 0。新段出现 = 段数超过它。
    pub base_segments: usize,
}

/// ffmpeg HLS 转码参数（视频 `-c:v copy` + AC-3→AAC）。
///
/// 视频始终 `-c:v copy` 纯转封装。
/// 音频分支：
/// - `Some("ac3")`：`-c:a aac -aac_coder fast -b:a 192k -ac 2` —— Chromium 无 AC-3 解码器，
///   必须转 AAC 才有声。`fast` 编码器 200-400× 实时。
/// - 其他（含 None / AAC / MP3 / Opus）：`-c:a copy`，零重编码开销。
///
/// HLS 参数：
/// - `-f hls`：HLS muxer
/// - `-hls_time 2`：目标段长 2s（`-c:v copy` 下按源关键帧就近对齐，段长可能略偏）
/// - `-hls_playlist_type event`：playlist 增量追加，转完写 `#EXT-X-ENDLIST`；
///   hls.js 会自动周期性 refetch 直到看到 ENDLIST
/// - `-hls_flags independent_segments+append_list+temp_file`：
///   - `independent_segments`：每段可独立解码（seek 时不用回退到前段的 keyframe）
///   - `append_list`：live 模式下 playlist 追加而不是重写全表
///   - `temp_file`：段先写到 `.tmp`、写完 rename——避免 hls.js 读到半段
/// - `-hls_list_size 0`：playlist 里保留全部段（不做滑窗，VOD 语义）
/// - `-hls_segment_filename`：段文件命名模板（basename 自动进 playlist 的 EXTINF）
fn transcode_args(
    input: &str,
    playlist_part: &str,
    segment_pattern: &str,
    audio_codec: Option<&str>,
    resume: Option<f64>,
) -> Vec<String> {
    let mut args: Vec<String> = vec!["-nostdin".into()];
    if let Some(start) = resume {
        // input-seek 必须在 -i 前（demuxer 快速 seek 到最近关键帧）
        args.push("-ss".into());
        args.push(format!("{start:.3}"));
    }
    args.extend([
        "-i".into(), input.into(),
        "-c:v".into(), "copy".into(),
    ]);
    match audio_codec {
        Some(c) if c.eq_ignore_ascii_case("ac3") => {
            args.extend([
                "-c:a".into(), "aac".into(),
                "-aac_coder".into(), "fast".into(),
                "-b:a".into(), "192k".into(),
                "-ac".into(), "2".into(),
            ]);
        }
        _ => {
            args.extend(["-c:a".into(), "copy".into()]);
        }
    }
    args.extend([
        "-f".into(), "hls".into(),
        "-hls_time".into(), "2".into(),
        "-hls_playlist_type".into(), "event".into(),
        "-hls_flags".into(), "independent_segments+append_list+temp_file".into(),
        "-hls_list_size".into(), "0".into(),
        "-hls_segment_filename".into(), segment_pattern.into(),
    ]);
    // 续转不传起始段号：`append_list` 会解析已有 playlist 并自动接续段号（实测）。
    args.extend([
        "-progress".into(), "pipe:2".into(),
        "-y".into(),
        playlist_part.into(),
    ]);
    args
}

/// 打开视频：若完整 playlist 已存在（有 `#EXT-X-ENDLIST`）则 `Ready`；
/// 否则清理旧的半吊子 `.m3u8` + `.ts` 段，后台 spawn ffmpeg **直接写** `.m3u8`（无 `.part`
/// 中间步），返回 `Started`。Tauri asset 协议只按精确文件名查找——若走 `.part`+rename
/// 中间步，前端拿到的 URL 会指向尚不存在的 `.m3u8` → 404。
/// 不阻塞等转码完成——调用方保存 handle、读 stderr 判退出；`playlist_is_complete` 判缓存。
///
/// `on_stage` 在各阶段开始时回调（`"check"` → `"probe"` → `"transcode"`），供前端显示初始化进度。
pub fn start_remux(
    cache_dir: &Path,
    path: &str,
    allow_resume: bool,
    on_stage: &dyn Fn(&str),
) -> AppResult<TranscodeStatus> {
    on_stage("check");
    ensure_ffmpeg_available()?;
    std::fs::create_dir_all(cache_dir).ok();
    on_stage("probe");
    let meta = probe_meta(path)?;
    // 两条及以上音轨：走分流（视频与每条音轨各成一路），播放时可无缝切换音轨。
    // 单音轨/无音轨：沿用原来的单流转码，缓存文件名不变，老缓存继续可用。
    if meta.audio.len() >= 2 {
        return start_split(cache_dir, path, &meta, on_stage);
    }
    let duration = meta.duration;
    let audio_codec = meta.first_audio_codec().map(str::to_string);
    let playlist = cached_playlist_path(cache_dir, path);
    // 复用：完整 playlist（含 `#EXT-X-ENDLIST`）才算 Ready；否则视为半吊子重转
    if playlist_is_complete(&playlist) {
        if !is_truncated(playlist_total_duration(&playlist), duration) {
            touch(&playlist);
            return Ok(TranscodeStatus::Ready(playlist, duration));
        }
        // 截断缓存：ENDLIST 之后无法续转，作废后从头转
        let _ = std::fs::remove_file(&playlist);
    }
    on_stage("transcode");
    let prefix = cache_prefix(path);
    // 若存在半吊子 playlist + 段：走**续转**（`-ss` seek 到已转时长处，
    // 配合 args 里的 `append_list` flag——ffmpeg 会往已有 playlist 追加新段而不是重写）
    // 续转起点 = 已有各段时长之和（段按关键帧切，并非整 2s，不能用"段数×2"估算）。
    let existing_segments = if playlist.is_file() {
        next_segment_number(&playlist, &prefix).unwrap_or(0)
    } else {
        0
    };
    let resume: Option<f64> = if allow_resume && existing_segments > 0 {
        Some(playlist_total_duration(&playlist))
    } else {
        None
    };
    if resume.is_none() {
        // 无可续转基础：清残留后从头转
        let _ = std::fs::remove_file(&playlist);
        remove_segments_by_prefix(cache_dir, &prefix);
    }
    // 清 `.ts.tmp` / `.m3u8.tmp` 残留（上次被杀在写一半——.tmp 尚未 rename 到最终名）
    remove_orphan_tmp_segments(cache_dir, &prefix);
    let seg_pattern = segment_pattern(cache_dir, &prefix);
    let child = ffmpeg_command()
        .args(transcode_args(
            path,
            &playlist.to_string_lossy(),
            &seg_pattern.to_string_lossy(),
            audio_codec.as_deref(),
            resume,
        ))
        .stdin(Stdio::null()) // `-nostdin`：不读键盘命令，停止一律由调用方 kill
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| AppError::Other(format!("ffmpeg spawn: {e}")))?;
    Ok(TranscodeStatus::Started(
        TranscodeHandle {
            child,
            final_path: playlist.clone(),
            progress_path: playlist.clone(),
            playlists: vec![playlist],
            cache_prefix: prefix,
            resume_offset: resume.unwrap_or(0.0),
            base_segments: if resume.is_some() { existing_segments as usize } else { 0 },
        },
        duration,
    ))
}

/// 多音轨分流的策略版本。只参与分流产物的缓存键，与单流的 `TRANSCODE_STRATEGY_VERSION`
/// 互不影响——改这里不会让单音轨视频的老缓存失效。
/// v1: 视频 `-c:v copy` 一路 + 每条音轨一路（AAC 原样 copy，其余转 AAC 192k 立体声）；
///     不支持续转（见 `start_split`）。
const SPLIT_STRATEGY_VERSION: u32 = 1;

/// 分流产物的缓存前缀（`collector_HASH`，形态与单流一致，LRU/清理/URI 协议都不用区分）。
pub fn split_cache_prefix(src: &str) -> String {
    let mut h = DefaultHasher::new();
    "split".hash(&mut h);
    SPLIT_STRATEGY_VERSION.hash(&mut h);
    src.hash(&mut h);
    format!("collector_{:016x}", h.finish())
}

/// 分流里某一路的 playlist 文件名。`variant` 是 `v`（视频）或 `a0` / `a1` …（音轨）。
fn variant_playlist_name(prefix: &str, variant: &str) -> String {
    format!("{prefix}_{variant}.m3u8")
}

/// 分流里一条音轨是否可以原样 copy。只有 AAC 可以：
/// hls.js 切换音轨时复用同一个音频缓冲区，各路编码必须一致，否则追加数据会失败
/// （实测 AAC 与 MP3 混用报 bufferAppendError）；AC-3 / DTS / TrueHD 等 Chromium 也解不了。
fn can_copy_audio(codec: &str) -> bool {
    codec.eq_ignore_ascii_case("aac")
}

/// 写给前端的主 playlist：声明一组可选音轨 + 一路纯视频。
/// hls.js 读到它后暴露 `audioTracks`，切换时只换音频那一路，视频缓冲不动。
/// 不写 CODECS：写错反而会被 hls.js 判成不兼容而拒播（实测）。
fn master_playlist(prefix: &str, tracks: &[AudioTrack]) -> String {
    let labels = audio_labels(tracks);
    let default = default_audio_index(tracks);
    let mut s = String::from("#EXTM3U\n#EXT-X-VERSION:6\n#EXT-X-INDEPENDENT-SEGMENTS\n");
    for (i, (track, label)) in tracks.iter().zip(&labels).enumerate() {
        // 属性值在引号里：去掉引号和换行，免得破坏格式
        let name: String = label
            .chars()
            .map(|c| if c == '"' { '\'' } else { c })
            .filter(|c| *c != '\r' && *c != '\n')
            .collect();
        s.push_str(&format!("#EXT-X-MEDIA:TYPE=AUDIO,GROUP-ID=\"aud\",NAME=\"{name}\""));
        let lang = track
            .language
            .as_deref()
            .filter(|l| l.chars().all(|c| c.is_ascii_alphanumeric() || c == '-'));
        if let Some(lang) = lang {
            s.push_str(&format!(",LANGUAGE=\"{lang}\""));
        }
        s.push_str(&format!(
            ",DEFAULT={},AUTOSELECT=YES,URI=\"{}\"\n",
            if i == default { "YES" } else { "NO" },
            variant_playlist_name(prefix, &format!("a{i}")),
        ));
    }
    s.push_str("#EXT-X-STREAM-INF:BANDWIDTH=8000000,AUDIO=\"aud\"\n");
    s.push_str(&variant_playlist_name(prefix, "v"));
    s.push('\n');
    s
}

/// 多音轨分流的 ffmpeg 参数：一个进程同时写视频一路 + 每条音轨一路。
///
/// - `-map 0:V:0`：第一条真正的视频流（大写 V 排除封面图这类附图流）。
/// - 每条音轨 `-map 0:a:N`；AAC 原样 copy，其余转 AAC 192k 立体声（`fast` 编码器）。
/// - `-var_stream_map "v:0,name:v a:0,name:a0 …"`：每条流各自成一路，
///   输出文件名里的 `%v` 被替换成 `v` / `a0` / `a1` …
/// - 其余 HLS 参数与单流一致，只是不带 `append_list`（分流不续转）。
/// - 不传 `-master_pl_name`：主 playlist 由我们自己写（要带中文标签）。
fn split_transcode_args(
    input: &str,
    playlist_pattern: &str,
    segment_pattern: &str,
    tracks: &[AudioTrack],
) -> Vec<String> {
    let mut args: Vec<String> = vec![
        "-nostdin".into(),
        "-i".into(), input.into(),
        "-map".into(), "0:V:0".into(),
    ];
    for i in 0..tracks.len() {
        args.extend(["-map".into(), format!("0:a:{i}")]);
    }
    args.extend(["-c:v".into(), "copy".into()]);
    let mut stream_map = String::from("v:0,name:v");
    for (i, track) in tracks.iter().enumerate() {
        if can_copy_audio(&track.codec) {
            args.extend([format!("-c:a:{i}"), "copy".into()]);
        } else {
            args.extend([
                format!("-c:a:{i}"), "aac".into(),
                format!("-aac_coder:a:{i}"), "fast".into(),
                format!("-b:a:{i}"), "192k".into(),
                format!("-ac:a:{i}"), "2".into(),
            ]);
        }
        stream_map.push_str(&format!(" a:{i},name:a{i}"));
    }
    args.extend([
        "-f".into(), "hls".into(),
        "-hls_time".into(), "2".into(),
        "-hls_playlist_type".into(), "event".into(),
        "-hls_flags".into(), "independent_segments+temp_file".into(),
        "-hls_list_size".into(), "0".into(),
        "-var_stream_map".into(), stream_map,
        "-hls_segment_filename".into(), segment_pattern.into(),
        "-progress".into(), "pipe:2".into(),
        "-y".into(),
        playlist_pattern.into(),
    ]);
    args
}

/// 删掉某个缓存组的全部文件（playlist、段、`.tmp` 残留）。分流不续转，重转前整组清掉。
fn remove_group(cache_dir: &Path, prefix: &str) {
    let rd = match std::fs::read_dir(cache_dir) { Ok(r) => r, Err(_) => return };
    for e in rd.filter_map(|e| e.ok()) {
        let name = match e.file_name().into_string() { Ok(n) => n, Err(_) => continue };
        let base = name.strip_suffix(".tmp").unwrap_or(&name);
        if extract_cache_prefix(base).as_deref() == Some(prefix) {
            let _ = std::fs::remove_file(e.path());
        }
    }
}

/// 多音轨视频的打开：完整缓存直接 `Ready`，否则整组清掉、从头分流转码。
///
/// 缓存完整 = 主 playlist 在 + 视频与每条音轨的 playlist 都有 `#EXT-X-ENDLIST` + 视频时长没被截断。
///
/// **不续转**：视频按关键帧切段、音频按 2 秒切段，各路已转时长并不相等；
/// 用同一个 `-ss` 续转会让音画在续接点之后错开。分流的半截缓存一律作废重转——
/// 起播仍然只等首段（约 2 秒），代价只是之前转过的部分要重新转一遍。
fn start_split(
    cache_dir: &Path,
    path: &str,
    meta: &ProbeMeta,
    on_stage: &dyn Fn(&str),
) -> AppResult<TranscodeStatus> {
    let prefix = split_cache_prefix(path);
    let master = cache_dir.join(format!("{prefix}.m3u8"));
    let video = cache_dir.join(variant_playlist_name(&prefix, "v"));
    let mut playlists = vec![video.clone()];
    for i in 0..meta.audio.len() {
        playlists.push(cache_dir.join(variant_playlist_name(&prefix, &format!("a{i}"))));
    }
    if master.is_file()
        && playlists.iter().all(|p| playlist_is_complete(p))
        && !is_truncated(playlist_total_duration(&video), meta.duration)
    {
        touch(&master);
        return Ok(TranscodeStatus::Ready(master, meta.duration));
    }
    on_stage("transcode");
    remove_group(cache_dir, &prefix);
    std::fs::write(&master, master_playlist(&prefix, &meta.audio))
        .map_err(|e| AppError::Other(format!("写入播放列表失败 {}: {e}", master.display())))?;
    let playlist_pattern = cache_dir.join(format!("{prefix}_%v.m3u8"));
    let segment_pattern = cache_dir.join(format!("{prefix}_%v_%05d.ts"));
    let child = ffmpeg_command()
        .args(split_transcode_args(
            path,
            &playlist_pattern.to_string_lossy(),
            &segment_pattern.to_string_lossy(),
            &meta.audio,
        ))
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| AppError::Other(format!("ffmpeg spawn: {e}")))?;
    Ok(TranscodeStatus::Started(
        TranscodeHandle {
            child,
            final_path: master,
            progress_path: video,
            playlists,
            cache_prefix: prefix,
            resume_offset: 0.0,
            base_segments: 0,
        },
        meta.duration,
    ))
}

/// 转码成功后调用：LRU 清理（`.m3u8` 已经写在最终位置、无需 rename）。
pub fn finalize_remux(cache_dir: &Path) {
    enforce_cache_limit(cache_dir, CACHE_LIMIT_BYTES);
}

/// 清 `collector_HASH_NNNNN.ts.tmp` 残留（上次被杀时段写一半、rename 到 .ts 前留下的）。
/// 只清 .tmp，不动已完成的 .ts。续转前调用避免与新一次 ffmpeg 的段号冲突。
fn remove_orphan_tmp_segments(cache_dir: &Path, prefix: &str) {
    let rd = match std::fs::read_dir(cache_dir) { Ok(r) => r, Err(_) => return };
    for e in rd.filter_map(|e| e.ok()) {
        let name = match e.file_name().into_string() { Ok(n) => n, Err(_) => continue };
        if name.starts_with(prefix) && (name.ends_with(".ts.tmp") || name.ends_with(".m3u8.tmp")) {
            let _ = std::fs::remove_file(e.path());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn cached_playlist_path_is_stable_and_m3u8() {
        let dir = Path::new("/cache");
        let a = cached_playlist_path(dir, "/movies/x.mkv");
        assert_eq!(a, cached_playlist_path(dir, "/movies/x.mkv"));
        assert!(a.starts_with("/cache"));
        assert!(a.to_string_lossy().ends_with(".m3u8"));
        assert_ne!(a, cached_playlist_path(dir, "/movies/y.mkv"));
        // 前缀 = playlist 文件名去 .m3u8
        let prefix = cache_prefix("/movies/x.mkv");
        assert_eq!(a.file_name().unwrap().to_str().unwrap(), format!("{prefix}.m3u8"));
        assert!(prefix.starts_with("collector_"));
    }

    #[test]
    fn playlist_duration_and_first_segment() {
        let tmp = tempfile::tempdir().unwrap();
        let p = tmp.path().join("x.m3u8");
        assert_eq!(playlist_total_duration(&p), 0.0, "不存在 → 0");
        assert_eq!(playlist_segment_count(&p), 0, "不存在 → 未就绪");
        std::fs::write(&p, "#EXTM3U\n#EXT-X-TARGETDURATION:2\n").unwrap();
        assert_eq!(playlist_segment_count(&p), 0, "只有头部 → 未就绪");
        std::fs::write(&p, "#EXTM3U\n#EXTINF:2.002000,\ns0.ts\n#EXTINF:1.5,\ns1.ts\n").unwrap();
        assert_eq!(playlist_segment_count(&p), 2);
        assert!((playlist_total_duration(&p) - 3.502).abs() < 1e-6);
    }

    #[test]
    fn truncated_cache_detection() {
        // 实测案例：源 6365s，缓存只有 4556s 却带 ENDLIST → 截断
        assert!(is_truncated(4556.2, 6365.4));
        // 容器时长与流时长的小出入不算截断
        assert!(!is_truncated(6363.0, 6365.4));
        assert!(!is_truncated(95.0, 100.0), "短片用 10s 绝对容差");
        // 探测不到源时长 → 不判截断
        assert!(!is_truncated(10.0, 0.0));
    }

    #[test]
    fn playlist_complete_detection() {
        let tmp = tempfile::tempdir().unwrap();
        let p = tmp.path().join("x.m3u8");
        // 转码中 playlist：只有 EXTINF，没有 ENDLIST
        std::fs::write(&p, "#EXTM3U\n#EXT-X-VERSION:6\n#EXTINF:2.0,\ns0.ts\n").unwrap();
        assert!(!playlist_is_complete(&p), "无 ENDLIST 视为未完成");
        // 完整 playlist：末尾有 ENDLIST
        std::fs::write(&p, "#EXTM3U\n#EXTINF:2.0,\ns0.ts\n#EXT-X-ENDLIST\n").unwrap();
        assert!(playlist_is_complete(&p), "有 ENDLIST 视为完整");
        // 无换行结尾也认（保险）
        std::fs::write(&p, "#EXTM3U\n#EXT-X-ENDLIST").unwrap();
        assert!(playlist_is_complete(&p));
        // 空文件：未完成
        std::fs::write(&p, "").unwrap();
        assert!(!playlist_is_complete(&p));
        // 不存在：未完成
        assert!(!playlist_is_complete(&tmp.path().join("nope.m3u8")));
    }

    #[test]
    fn extract_cache_prefix_matches_all_shapes() {
        assert_eq!(extract_cache_prefix("collector_ab12.m3u8"), Some("collector_ab12".into()));
        assert_eq!(extract_cache_prefix("collector_ab12_00000.ts"), Some("collector_ab12".into()));
        assert_eq!(extract_cache_prefix("collector_ab12_99999.ts"), Some("collector_ab12".into()));
        // 老的 .m3u8.part 已不再产生：不识别
        assert_eq!(extract_cache_prefix("collector_ab12.m3u8.part"), None);
        assert_eq!(extract_cache_prefix("other.ts"), None);
        assert_eq!(extract_cache_prefix("collector_ab12.mp4"), None); // 旧 v1-v5 产物
        assert_eq!(extract_cache_prefix("collector_ab12_abc.ts"), None); // 段号必须全数字
        assert_eq!(extract_cache_prefix("collector.m3u8"), None); // 没有 _HASH
    }

    #[test]
    fn transcode_args_ac3_transcode_to_aac_stereo_with_hls() {
        // AC-3：视频 copy，音频转 AAC 192k 立体声 + fast 编码器；HLS 封装
        let args = transcode_args("/in.mkv", "/out.m3u8.part", "/seg_%05d.ts", Some("ac3"), None);
        let joined = args.join(" ");
        assert!(joined.contains("-c:v copy"));
        assert!(joined.contains("-c:a aac"));
        assert!(joined.contains("-aac_coder fast"));
        assert!(joined.contains("-b:a 192k"));
        assert!(joined.contains("-ac 2"));
        assert!(!joined.contains("-c:a copy"));
        assert!(joined.contains("-f hls"));
        assert!(joined.contains("-hls_time 2"));
        assert!(joined.contains("-hls_playlist_type event"));
        assert!(joined.contains("-hls_flags independent_segments+append_list+temp_file"));
        assert!(joined.contains("-hls_list_size 0"));
        assert!(joined.contains("-hls_segment_filename /seg_%05d.ts"));
        assert!(joined.contains("-progress pipe:2"));
        assert!(joined.ends_with("/out.m3u8.part"));
    }

    #[test]
    fn transcode_args_ac3_case_insensitive() {
        let args = transcode_args("/in.mkv", "/out.m3u8.part", "/seg_%05d.ts", Some("AC3"), None);
        let joined = args.join(" ");
        assert!(joined.contains("-c:a aac"));
        assert!(!joined.contains("-c:a copy"));
    }

    #[test]
    fn transcode_args_non_ac3_pure_copy_with_hls() {
        // 非 AC-3：音频 copy；封装同为 HLS（让 copy 分支也能秒开）
        for codec in [Some("aac"), Some("mp3"), Some("opus"), Some("vorbis"), Some("eac3"), Some("dts"), None] {
            let args = transcode_args("/in.mkv", "/out.m3u8.part", "/seg_%05d.ts", codec, None);
            let joined = args.join(" ");
            assert!(joined.contains("-c:v copy"), "codec={codec:?}");
            assert!(joined.contains("-c:a copy"), "codec={codec:?}");
            assert!(!joined.contains("-c:a aac"), "codec={codec:?}");
            assert!(!joined.contains("-aac_coder"), "codec={codec:?}");
            assert!(!joined.contains("-b:a"), "codec={codec:?}");
            assert!(joined.contains("-f hls"), "codec={codec:?}");
            assert!(joined.contains("-hls_time 2"), "codec={codec:?}");
            assert!(joined.contains("-hls_segment_filename /seg_%05d.ts"), "codec={codec:?}");
            assert!(joined.contains("-progress pipe:2"), "codec={codec:?}");
            assert!(joined.ends_with("/out.m3u8.part"), "codec={codec:?}");
        }
    }

    #[test]
    fn transcode_args_resume_adds_ss_only() {
        // 续转：`-ss` 在 `-i` 前；段号由 append_list 自行接续，不传任何起始段号
        let args = transcode_args(
            "/in.mkv", "/out.m3u8", "/seg_%05d.ts",
            Some("aac"), Some(42.0),
        );
        // 找 -ss 的位置必须早于 -i
        let ss_pos = args.iter().position(|a| a == "-ss").expect("-ss present");
        let i_pos = args.iter().position(|a| a == "-i").expect("-i present");
        assert!(ss_pos < i_pos, "-ss must precede -i for input seek");
        assert_eq!(args.get(ss_pos + 1).map(|s| s.as_str()), Some("42.000"));
        // `-hls_start_number` 不是 ffmpeg 选项（会直接报错退出）；`-start_number` 则会与
        // append_list 的自动接续叠加导致段号跳号——两者都不能出现
        assert!(!args.iter().any(|a| a.contains("start_number")));
        // append_list 仍在 hls_flags 里（用来追加已有 playlist）
        let joined = args.join(" ");
        assert!(joined.contains("append_list"));
    }

    #[test]
    fn transcode_args_fresh_has_no_resume_flags() {
        // 无 resume：不出现 -ss
        let args = transcode_args("/in.mkv", "/out.m3u8", "/seg_%05d.ts", None, None);
        assert!(!args.iter().any(|a| a == "-ss"));
        assert!(!args.iter().any(|a| a.contains("start_number")));
    }

    #[test]
    fn next_segment_number_parses_playlist() {
        let tmp = tempfile::tempdir().unwrap();
        let p = tmp.path().join("collector_ab.m3u8");
        std::fs::write(&p, "\
#EXTM3U\n#EXT-X-VERSION:6\n\
#EXTINF:2.0,\ncollector_ab_00000.ts\n\
#EXTINF:2.0,\ncollector_ab_00001.ts\n\
#EXTINF:2.0,\ncollector_ab_00002.ts\n").unwrap();
        assert_eq!(next_segment_number(&p, "collector_ab"), Some(3));
        // 空 playlist（只有 header）→ None
        std::fs::write(&p, "#EXTM3U\n#EXT-X-VERSION:6\n").unwrap();
        assert_eq!(next_segment_number(&p, "collector_ab"), None);
        // 前缀不匹配 → None（跳过其他组）
        std::fs::write(&p, "#EXTM3U\n#EXTINF:2.0,\ncollector_XX_00000.ts\n").unwrap();
        assert_eq!(next_segment_number(&p, "collector_ab"), None);
    }

    #[test]
    fn cache_key_changes_when_strategy_version_bumps() {
        // 策略版本参与哈希：升级后老缓存自然不再命中，无需手动清缓存。
        let dir = Path::new("/cache");
        let current = cached_playlist_path(dir, "/movies/x.mkv");
        // 手工重算：若 VERSION=1（老版）则哈希与当前不同
        let legacy = {
            let mut h = DefaultHasher::new();
            1u32.hash(&mut h);
            "/movies/x.mkv".hash(&mut h);
            dir.join(format!("collector_{:016x}.m3u8", h.finish()))
        };
        assert_ne!(current, legacy, "current strategy hash should differ from v1's");
    }

    #[test]
    fn lru_deletes_oldest_group_over_limit() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path();
        // 三组：a/b/c，每组 = 1 个 .m3u8 (100B) + 2 个 .ts (各 900B) = 1900B/组
        for (i, p) in ["collector_a", "collector_b", "collector_c"].iter().enumerate() {
            let playlist = dir.join(format!("{p}.m3u8"));
            let seg0 = dir.join(format!("{p}_00000.ts"));
            let seg1 = dir.join(format!("{p}_00001.ts"));
            std::fs::File::create(&playlist).unwrap().write_all(&[0u8; 100]).unwrap();
            std::fs::File::create(&seg0).unwrap().write_all(&[0u8; 900]).unwrap();
            std::fs::File::create(&seg1).unwrap().write_all(&[0u8; 900]).unwrap();
            let t = filetime::FileTime::from_unix_time(1000 + i as i64, 0);
            for f in [&playlist, &seg0, &seg1] {
                filetime::set_file_mtime(f, t).unwrap();
            }
        }
        // 5700 → 需删到 ≤4000：至少删最旧的一组 a（含它的所有段）
        enforce_cache_limit(dir, 4000);
        assert!(!dir.join("collector_a.m3u8").exists(), "playlist a 应被删");
        assert!(!dir.join("collector_a_00000.ts").exists(), "a 段 0 应被删");
        assert!(!dir.join("collector_a_00001.ts").exists(), "a 段 1 应被删");
        assert!(dir.join("collector_b.m3u8").exists());
        assert!(dir.join("collector_c.m3u8").exists());
    }

    #[test]
    fn cache_used_and_clear_touch_current_and_legacy_formats() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path();
        // 当前 v7 格式
        std::fs::File::create(dir.join("collector_x.m3u8")).unwrap().write_all(&[0u8; 100]).unwrap();
        std::fs::File::create(dir.join("collector_x_00000.ts")).unwrap().write_all(&[0u8; 500]).unwrap();
        std::fs::File::create(dir.join("collector_x_00001.ts.tmp")).unwrap().write_all(&[0u8; 200]).unwrap();
        // 历史格式：v1-v5 单文件 mp4
        std::fs::File::create(dir.join("collector_legacy1.mp4")).unwrap().write_all(&[0u8; 400]).unwrap();
        std::fs::File::create(dir.join("collector_legacy2.mp4.part")).unwrap().write_all(&[0u8; 250]).unwrap();
        // v6 半吊 playlist
        std::fs::File::create(dir.join("collector_legacy3.m3u8.part")).unwrap().write_all(&[0u8; 150]).unwrap();
        // 无关文件保留
        std::fs::File::create(dir.join("other.txt")).unwrap().write_all(&[0u8; 999]).unwrap();
        // 100+500+200+400+250+150 = 1600
        assert_eq!(cache_used_bytes(dir), 1600);
        assert_eq!(clear_cache(dir), 6);
        assert!(!dir.join("collector_x.m3u8").exists());
        assert!(!dir.join("collector_x_00000.ts").exists());
        assert!(!dir.join("collector_x_00001.ts.tmp").exists());
        assert!(!dir.join("collector_legacy1.mp4").exists());
        assert!(!dir.join("collector_legacy2.mp4.part").exists());
        assert!(!dir.join("collector_legacy3.m3u8.part").exists());
        assert!(dir.join("other.txt").exists());
        assert_eq!(cache_used_bytes(dir), 0);
    }

    #[test]
    fn remove_segments_by_prefix_only_touches_matching_ts() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path();
        std::fs::write(dir.join("collector_ab_00000.ts"), b"x").unwrap();
        std::fs::write(dir.join("collector_ab_00001.ts"), b"x").unwrap();
        std::fs::write(dir.join("collector_ab.m3u8"), b"x").unwrap(); // 保留：不删 playlist
        std::fs::write(dir.join("collector_cd_00000.ts"), b"x").unwrap(); // 保留：其他组
        std::fs::write(dir.join("other.ts"), b"x").unwrap(); // 保留：非本模块
        remove_segments_by_prefix(dir, "collector_ab");
        assert!(!dir.join("collector_ab_00000.ts").exists());
        assert!(!dir.join("collector_ab_00001.ts").exists());
        assert!(dir.join("collector_ab.m3u8").exists());
        assert!(dir.join("collector_cd_00000.ts").exists());
        assert!(dir.join("other.ts").exists());
    }

    #[test]
    fn ffmpeg_hint_mentions_reinstall() {
        let h = ffmpeg_missing_hint();
        assert!(h.contains("内置"));
        assert!(h.contains("重新下载并安装 Collector"));
        assert!(h.contains("ffmpeg"));
        assert!(!h.contains("ffprobe"), "已不再内置 ffprobe");
    }

    #[test]
    fn probe_output_parses_duration_and_first_audio_codec() {
        // 实测输出（安娜贝尔3）。Matroska 元数据里的 `DURATION` 标签不能误认成时长。
        let text = concat!(
            "Input #0, matroska,webm, from 'a.mkv':\n",
            "  Metadata:\n",
            "    DURATION        : 01:46:05.408000000\n",
            "  Duration: 01:46:05.41, start: 0.000000, bitrate: 4887 kb/s\n",
            "  Stream #0:0(eng): Video: h264 (Main), yuv420p(progressive), 1280x720\n",
            "  Stream #0:1(eng): Audio: ac3, 48000 Hz, stereo, fltp, 256 kb/s (default)\n",
            "  Stream #0:2(chi): Audio: aac (LC) (mp4a / 0x6134706D), 48000 Hz\n",
            "At least one output file must be specified\n",
        );
        let meta = parse_probe_output(text).expect("parsed");
        let d = meta.duration;
        assert!((d - 6365.41).abs() < 1e-6, "d={d}");
        assert_eq!(meta.first_audio_codec(), Some("ac3"), "取第一条音轨");
        assert_eq!(meta.audio.len(), 2, "两条音轨都要认出来");
    }

    #[test]
    fn probe_output_edge_cases() {
        // 无音轨
        let meta = parse_probe_output("  Duration: 00:00:10.50, start: 0\n  Stream #0:0: Video: h264\n").unwrap();
        assert!((meta.duration - 10.5).abs() < 1e-9);
        assert!(meta.audio.is_empty());
        // 时长未知 → 0
        let meta = parse_probe_output("  Duration: N/A, bitrate: N/A\n").unwrap();
        assert_eq!(meta.duration, 0.0);
        // 编码名带下划线/数字，统一小写
        let meta = parse_probe_output("  Duration: 00:01:00.00\n  Stream #0:1: Audio: pcm_s16LE, 48000 Hz\n").unwrap();
        assert_eq!(meta.first_audio_codec(), Some("pcm_s16le"));
        // 打不开的输入：没有 Duration 行
        assert!(parse_probe_output("[in#0] Error opening input: Invalid data found\n").is_none());
    }

    /// 三条音轨的真实 `ffmpeg -i` 输出（含章节与字幕流，它们的 title 不能记到音轨上）。
    const MULTI_AUDIO_PROBE: &str = concat!(
        "Input #0, matroska,webm, from 'm.mkv':\n",
        "  Metadata:\n",
        "    title           : 影片标题\n",
        "  Duration: 00:00:40.03, start: 0.000000, bitrate: 1445 kb/s\n",
        "  Chapters:\n",
        "    Chapter #0:0: start 0.000000, end 20.000000\n",
        "      Metadata:\n",
        "        title           : Chapter 1\n",
        "  Stream #0:0: Video: h264 (High), yuv420p(tv, progressive), 1920x1080, 24 fps\n",
        "      Metadata:\n",
        "        title           : 视频流标题\n",
        "  Stream #0:1(eng): Audio: ac3, 48000 Hz, 5.1(side), fltp, 384 kb/s (default)\n",
        "      Metadata:\n",
        "        title           : English 5.1\n",
        "        BPS             : 384000\n",
        "  Stream #0:2(chi): Audio: aac (LC), 44100 Hz, stereo, fltp\n",
        "      Metadata:\n",
        "        title           : 国语\n",
        "  Stream #0:3: Audio: dts (DTS-HD MA), 48000 Hz, 7.1, s32p (24 bit)\n",
        "  Stream #0:4(chi): Subtitle: ass (default)\n",
        "      Metadata:\n",
        "        title           : 简体中文\n",
        "At least one output file must be specified\n",
    );

    #[test]
    fn probe_output_lists_every_audio_track() {
        let meta = parse_probe_output(MULTI_AUDIO_PROBE).expect("parsed");
        assert!((meta.duration - 40.03).abs() < 1e-6);
        assert_eq!(meta.audio, vec![
            AudioTrack {
                codec: "ac3".into(), language: Some("eng".into()), title: Some("English 5.1".into()),
                channels: Some("5.1(side)".into()), default: true,
            },
            AudioTrack {
                codec: "aac".into(), language: Some("chi".into()), title: Some("国语".into()),
                channels: Some("stereo".into()), default: false,
            },
            // 没有语言、没有标题；后面字幕流的 title 不能被它捡走
            AudioTrack {
                codec: "dts".into(), language: None, title: None,
                channels: Some("7.1".into()), default: false,
            },
        ]);
    }

    #[test]
    fn probe_audio_stream_variants() {
        // 带流 ID 的写法、und 语言、声道写在行尾并带 (default)
        let t = parse_audio_stream("Stream #0:1[0x1100](JPN)", "eac3, 48000 Hz, stereo (default)").unwrap();
        assert_eq!(t.language.as_deref(), Some("jpn"));
        assert_eq!(t.channels.as_deref(), Some("stereo"));
        assert!(t.default);
        let t = parse_audio_stream("Stream #0:2(und)", "mp3 (mp3float), 44100 Hz, mono, fltp, 64 kb/s").unwrap();
        assert_eq!(t.language, None, "und = 未标语言");
        assert_eq!(t.codec, "mp3");
        assert_eq!(t.channels.as_deref(), Some("mono"));
        assert!(!t.default);
        // 没有采样率字段 → 声道未知，不崩
        let t = parse_audio_stream("Stream #0:1", "truehd").unwrap();
        assert_eq!((t.channels, t.language), (None, None));
        assert!(parse_audio_stream("Stream #0:1", "").is_none());
    }

    fn track(codec: &str, lang: Option<&str>, title: Option<&str>, ch: Option<&str>, default: bool) -> AudioTrack {
        AudioTrack {
            codec: codec.into(),
            language: lang.map(Into::into),
            title: title.map(Into::into),
            channels: ch.map(Into::into),
            default,
        }
    }

    #[test]
    fn audio_labels_are_readable_and_unique() {
        let tracks = [
            track("ac3", Some("eng"), Some("English 5.1"), Some("5.1(side)"), true),
            track("aac", Some("chi"), None, Some("stereo"), false),
            track("dts", None, None, Some("7.1"), false),
            track("aac", Some("chi"), Some("中文配音"), Some("stereo"), false), // 标题已含语言名
            track("truehd", Some("xxx"), Some("导演评论"), None, false),         // 不认识的语言代码
            track("dts", Some("fre"), Some("DTS-HD MA"), Some("5.1(side)"), false), // 标题没提语言 → 补上
            track("aac", Some("chi"), None, Some("stereo"), false),              // 与第 2 条重名
        ];
        assert_eq!(audio_labels(&tracks), [
            "English 5.1 (AC3 5.1)",
            "中文 (AAC 2.0) #2",
            "音轨 3 (DTS 7.1)",
            "中文配音 (AAC 2.0)",
            "导演评论 (TrueHD)",
            "法语 · DTS-HD MA (DTS 5.1)",
            "中文 (AAC 2.0) #7",
        ]);
    }

    #[test]
    fn default_audio_is_flagged_track_or_first() {
        let a = track("aac", None, None, None, false);
        let d = track("aac", None, None, None, true);
        assert_eq!(default_audio_index(&[a.clone(), d.clone(), d.clone()]), 1);
        assert_eq!(default_audio_index(&[a.clone(), a.clone()]), 0);
    }

    #[test]
    fn master_playlist_declares_audio_group_and_video() {
        let tracks = [
            track("ac3", Some("eng"), Some("English \"5.1\"\n"), Some("5.1(side)"), false),
            track("aac", Some("chi"), Some("国语"), Some("stereo"), true),
            track("dts", Some("bad lang!"), None, None, false),
        ];
        let m = master_playlist("collector_ab", &tracks);
        assert_eq!(m, concat!(
            "#EXTM3U\n#EXT-X-VERSION:6\n#EXT-X-INDEPENDENT-SEGMENTS\n",
            // 标题里的引号换成单引号、换行去掉
            "#EXT-X-MEDIA:TYPE=AUDIO,GROUP-ID=\"aud\",NAME=\"English '5.1' (AC3 5.1)\",LANGUAGE=\"eng\",DEFAULT=NO,AUTOSELECT=YES,URI=\"collector_ab_a0.m3u8\"\n",
            // 源文件标的默认音轨 → DEFAULT=YES
            "#EXT-X-MEDIA:TYPE=AUDIO,GROUP-ID=\"aud\",NAME=\"国语 (AAC 2.0)\",LANGUAGE=\"chi\",DEFAULT=YES,AUTOSELECT=YES,URI=\"collector_ab_a1.m3u8\"\n",
            // 语言代码不合规就不写 LANGUAGE
            "#EXT-X-MEDIA:TYPE=AUDIO,GROUP-ID=\"aud\",NAME=\"音轨 3 (DTS)\",DEFAULT=NO,AUTOSELECT=YES,URI=\"collector_ab_a2.m3u8\"\n",
            "#EXT-X-STREAM-INF:BANDWIDTH=8000000,AUDIO=\"aud\"\n",
            "collector_ab_v.m3u8\n",
        ));
        assert!(!m.contains("CODECS"), "不写 CODECS，写错会被 hls.js 拒播");
    }

    #[test]
    fn split_args_map_every_track_and_only_copy_aac() {
        let tracks = [
            track("ac3", None, None, None, true),
            track("aac", None, None, None, false),
            track("mp3", None, None, None, false),
        ];
        let args = split_transcode_args("/in.mkv", "/c/p_%v.m3u8", "/c/p_%v_%05d.ts", &tracks);
        let joined = args.join(" ");
        // 视频一路 + 三条音轨各一路
        assert!(joined.contains("-map 0:V:0 -map 0:a:0 -map 0:a:1 -map 0:a:2"), "{joined}");
        assert!(joined.contains("-c:v copy"));
        // AAC 原样 copy；AC-3、MP3 都转 AAC 立体声（各路编码必须一致）
        assert!(joined.contains("-c:a:0 aac -aac_coder:a:0 fast -b:a:0 192k -ac:a:0 2"), "{joined}");
        assert!(joined.contains("-c:a:1 copy"), "{joined}");
        assert!(joined.contains("-c:a:2 aac -aac_coder:a:2 fast -b:a:2 192k -ac:a:2 2"), "{joined}");
        let map_pos = args.iter().position(|a| a == "-var_stream_map").expect("var_stream_map");
        assert_eq!(args[map_pos + 1], "v:0,name:v a:0,name:a0 a:1,name:a1 a:2,name:a2");
        assert!(joined.contains("-f hls -hls_time 2 -hls_playlist_type event"));
        // 分流不续转：不带 append_list，也没有 -ss
        assert!(joined.contains("-hls_flags independent_segments+temp_file"));
        assert!(!joined.contains("append_list"));
        assert!(!args.iter().any(|a| a == "-ss"));
        assert!(joined.contains("-hls_segment_filename /c/p_%v_%05d.ts"));
        assert!(joined.contains("-progress pipe:2"));
        assert!(joined.ends_with("/c/p_%v.m3u8"));
    }

    #[test]
    fn extract_cache_prefix_understands_split_variants() {
        for name in [
            "collector_ab12.m3u8",          // 主 playlist
            "collector_ab12_v.m3u8",        // 视频
            "collector_ab12_a0.m3u8",       // 音轨
            "collector_ab12_a12.m3u8",
            "collector_ab12_v_00000.ts",
            "collector_ab12_a0_00042.ts",
            "collector_ab12_a12_99999.ts",
        ] {
            assert_eq!(extract_cache_prefix(name), Some("collector_ab12".into()), "{name}");
        }
        // 哈希本身长得像 `a123…`：不是音轨后缀，不能被剥掉
        assert_eq!(
            extract_cache_prefix("collector_a123456789012345.m3u8"),
            Some("collector_a123456789012345".into()));
        assert_eq!(
            extract_cache_prefix("collector_a123456789012345_00007.ts"),
            Some("collector_a123456789012345".into()));
        // `_a` 后面没有数字、`_vv` 都不是流后缀
        assert_eq!(extract_cache_prefix("collector_ab12_a.m3u8"), Some("collector_ab12_a".into()));
        assert_eq!(extract_cache_prefix("collector_ab12_vv.m3u8"), Some("collector_ab12_vv".into()));
    }

    #[test]
    fn split_and_single_stream_caches_do_not_collide() {
        // 同一个源文件：单流与分流的缓存前缀不同；分流前缀同样是 collector_ + 16 位十六进制
        let single = cache_prefix("/movies/x.mkv");
        let split = split_cache_prefix("/movies/x.mkv");
        assert_ne!(single, split);
        assert_eq!(split, split_cache_prefix("/movies/x.mkv"));
        assert_ne!(split, split_cache_prefix("/movies/y.mkv"));
        let hash = split.strip_prefix("collector_").expect("prefix");
        assert_eq!(hash.len(), 16);
        assert!(hash.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn remove_group_clears_whole_split_group_only() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path();
        let mine = [
            "collector_ab.m3u8", "collector_ab_v.m3u8", "collector_ab_a0.m3u8",
            "collector_ab_v_00000.ts", "collector_ab_a0_00000.ts", "collector_ab_a1_00003.ts",
            "collector_ab_v_00001.ts.tmp", "collector_ab_a0.m3u8.tmp",
        ];
        let others = ["collector_abc.m3u8", "collector_abc_v_00000.ts", "collector_cd_00000.ts", "other.ts"];
        for n in mine.iter().chain(&others) {
            std::fs::write(dir.join(n), b"x").unwrap();
        }
        remove_group(dir, "collector_ab");
        for n in mine {
            assert!(!dir.join(n).exists(), "{n} 应被删");
        }
        for n in others {
            assert!(dir.join(n).exists(), "{n} 不属于这一组，应保留");
        }
    }

    #[test]
    fn lru_treats_split_group_as_one_unit() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path();
        // 旧的分流组（主 playlist + 视频 + 两条音轨）与新的单流组
        let old = [
            "collector_old.m3u8", "collector_old_v.m3u8", "collector_old_a0.m3u8", "collector_old_a1.m3u8",
            "collector_old_v_00000.ts", "collector_old_a0_00000.ts", "collector_old_a1_00000.ts",
        ];
        let new = ["collector_new.m3u8", "collector_new_00000.ts"];
        for (names, secs) in [(&old[..], 1000), (&new[..], 2000)] {
            for n in names {
                let p = dir.join(n);
                std::fs::File::create(&p).unwrap().write_all(&[0u8; 500]).unwrap();
                filetime::set_file_mtime(&p, filetime::FileTime::from_unix_time(secs, 0)).unwrap();
            }
        }
        // 共 4500 字节，上限 2000：最旧的分流组必须整组删掉，不能只删一部分
        enforce_cache_limit(dir, 2000);
        for n in old {
            assert!(!dir.join(n).exists(), "{n} 应随整组被删");
        }
        for n in new {
            assert!(dir.join(n).exists(), "{n} 应保留");
        }
    }

    /// 真实 ffmpeg 跑一遍分流（手动运行）：
    /// `SPLIT_IN=<多音轨视频> SPLIT_OUT=<空目录> cargo test --lib split_real_ffmpeg -- --ignored --nocapture`
    /// 需要 PATH 里有 ffmpeg。
    #[test]
    #[ignore]
    fn split_real_ffmpeg() {
        let input = std::env::var("SPLIT_IN").expect("SPLIT_IN");
        let out = PathBuf::from(std::env::var("SPLIT_OUT").expect("SPLIT_OUT"));
        let stages = std::cell::RefCell::new(Vec::<String>::new());
        let on_stage = |s: &str| stages.borrow_mut().push(s.to_string());
        let status = start_remux(&out, &input, true, &on_stage).expect("start");
        let TranscodeStatus::Started(handle, duration) = status else { panic!("首次应为 Started") };
        assert_eq!(handle.playlists.len(), 1 + probe_meta(&input).unwrap().audio.len());
        assert_ne!(handle.final_path, handle.progress_path, "入口是主 playlist，进度看视频那路");
        let output = handle.child.wait_with_output().expect("wait");
        assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
        for p in &handle.playlists {
            assert!(playlist_is_complete(p), "{} 应完整", p.display());
            assert!(playlist_segment_count(p) > 0);
        }
        let video_secs = playlist_total_duration(&handle.progress_path);
        assert!((video_secs - duration).abs() < 3.0, "video={video_secs} source={duration}");
        println!("stages={:?}\nmaster:\n{}", stages.borrow(), std::fs::read_to_string(&handle.final_path).unwrap());
        // 再开一次：完整缓存直接 Ready，入口还是那个主 playlist
        match start_remux(&out, &input, true, &|_| {}).expect("reopen") {
            TranscodeStatus::Ready(path, _) => assert_eq!(path, handle.final_path),
            TranscodeStatus::Started(mut h, _) => { let _ = h.child.kill(); panic!("完整缓存应直接复用"); }
        }
        // 模拟半截缓存（去掉一条音轨的 ENDLIST）：不续转，整组重来
        let a0 = &handle.playlists[1];
        let text = std::fs::read_to_string(a0).unwrap().replace("#EXT-X-ENDLIST", "");
        std::fs::write(a0, text).unwrap();
        match start_remux(&out, &input, true, &|_| {}).expect("restart") {
            TranscodeStatus::Started(h, _) => {
                assert_eq!((h.resume_offset, h.base_segments), (0.0, 0));
                let o = h.child.wait_with_output().expect("wait");
                assert!(o.status.success());
                assert!(h.playlists.iter().all(|p| playlist_is_complete(p)));
            }
            TranscodeStatus::Ready(..) => panic!("半截缓存不能当完整缓存用"),
        }
    }
}
