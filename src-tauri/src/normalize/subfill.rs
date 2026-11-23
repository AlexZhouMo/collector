//! 下载字幕（工具箱「下载字幕」）：扫描数据库里的所有视频，输入目录下缺少对应 .ass 的，自动从射手网(伪) assrt.net 下载。
//!
//! 对应规则与校准一致：<输入目录>/<category>/<category_path>/<title>.ass。
//! 射手网 API 配额 5 次/分钟（search 与 detail 各算一次），由 [`Limiter`] 在后台排队限速；
//! 字幕文件本身的下载地址不在配额内，不限速。只要 ASS：搜索结果按 subtype 过滤，
//! 最终落盘的文件名也必须是 .ass。

use crate::error::{AppError, AppResult};
use crate::poster::parse::clean_title;
use serde::Serialize;
use std::collections::VecDeque;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

/// 先走 https；网络超时/连不上时换 http 再试（两者同一台服务器，偶尔只有一个通）
const API_BASES: [&str; 2] = ["https://api.assrt.net/v1", "http://api.assrt.net/v1"];
/// 网络错误（超时、连不上）时同一请求最多试几次
const NET_ATTEMPTS: usize = 3;
const USER_AGENT: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) collector/1.0";
/// 每分钟最多调用 API 次数
const QUOTA_PER_MIN: usize = 5;
/// 每个视频最多查看几条候选字幕的详情（每条耗一次配额）
const MAX_DETAILS: usize = 3;
/// 连续多少次 API 调用失败就整批中止（多半是 Key 无效或服务不可用）
const MAX_CONSECUTIVE_API_ERRORS: usize = 3;
/// 单个字幕文件/压缩包下载上限
const MAX_DOWNLOAD_BYTES: u64 = 30 * 1024 * 1024;

/// 一个待补的视频
#[derive(Debug, Clone)]
pub struct MediaRow {
    pub category: String,
    pub category_path: String,
    pub title: String,
}

#[derive(Debug, Serialize, Default)]
pub struct FillFailure {
    pub file: String,
    pub reason: String,
}

#[derive(Debug, Serialize, Default)]
pub struct FillReport {
    /// 数据库视频总数
    pub total: usize,
    /// 原本就有字幕的数量
    pub existing: usize,
    /// 本次下载成功的文件（相对输入目录）
    pub downloaded: Vec<String>,
    /// 射手网上没找到 ASS 字幕的
    pub not_found: Vec<String>,
    /// 出错的
    pub failed: Vec<FillFailure>,
    /// 整批中止原因（如 Key 无效），正常完成为 None
    pub aborted: Option<String>,
}

/// 视频对应的字幕相对路径（相对输入目录，用 / 分隔，供报告显示）
pub fn subtitle_rel(row: &MediaRow) -> String {
    let mut parts = vec![row.category.as_str()];
    parts.extend(row.category_path.split('/').filter(|s| !s.is_empty()));
    let file = format!("{}.ass", row.title);
    let mut s = parts.join("/");
    s.push('/');
    s.push_str(&file);
    s
}

fn subtitle_abs(in_dir: &Path, row: &MediaRow) -> PathBuf {
    let mut p = in_dir.join(&row.category);
    for seg in row.category_path.split('/').filter(|s| !s.is_empty()) {
        p.push(seg);
    }
    p.push(format!("{}.ass", row.title));
    p
}

// ---------------- 限速 ----------------

/// 滑动窗口限速：任意 60 秒内最多 QUOTA_PER_MIN 次。
struct Limiter {
    times: VecDeque<Instant>,
    window: Duration,
}

impl Limiter {
    fn new() -> Self {
        Self { times: VecDeque::new(), window: Duration::from_secs(60) }
    }
    fn acquire(&mut self) {
        loop {
            let now = Instant::now();
            while let Some(&t) = self.times.front() {
                if now.duration_since(t) >= self.window { self.times.pop_front(); } else { break; }
            }
            if self.times.len() < QUOTA_PER_MIN {
                self.times.push_back(now);
                return;
            }
            // 等到最早一次滑出窗口，多留 1 秒余量（服务端计时与本地有偏差）
            let wait = self.window - now.duration_since(self.times[0]) + Duration::from_secs(1);
            std::thread::sleep(wait);
        }
    }
}

// ---------------- 搜索词与匹配 ----------------

/// 剧集集号：标题开头的 SxxEyy 或 Eyy（如 S03E05.人与武器的对抗、E01.怨屋本铺）。
fn parse_episode(title: &str) -> Option<(Option<u32>, u32)> {
    let t = title.trim();
    let upper = t.to_ascii_uppercase();
    let b = upper.as_bytes();
    let digits = |from: usize| -> (usize, Option<u32>) {
        let mut i = from;
        while i < b.len() && b[i].is_ascii_digit() { i += 1; }
        (i, upper[from..i].parse().ok())
    };
    if b.first() == Some(&b'S') {
        let (i, s) = digits(1);
        if s.is_some() && b.get(i) == Some(&b'E') {
            let (_, e) = digits(i + 1);
            return e.map(|e| (s, e));
        }
    } else if b.first() == Some(&b'E') {
        let (_, e) = digits(1);
        return e.map(|e| (None, e));
    }
    None
}

/// 「第N季」→ N
fn parse_season_seg(seg: &str) -> Option<u32> {
    let rest = seg.strip_prefix('第')?;
    let idx = rest.find('季')?;
    rest[..idx].parse().ok()
}

#[derive(Debug, Clone)]
struct Target {
    query: String,
    year: Option<u32>,
    /// 剧集：(季, 集)
    episode: Option<(u32, u32)>,
}

fn build_target(row: &MediaRow) -> Target {
    if row.category == "剧集" {
        let segs: Vec<&str> = row.category_path.split('/').filter(|s| !s.is_empty()).collect();
        let (show, season_dir) = match segs.last().and_then(|s| parse_season_seg(s)) {
            Some(n) => (segs.get(segs.len().saturating_sub(2)).copied().unwrap_or(""), Some(n)),
            None => (segs.last().copied().unwrap_or(""), None),
        };
        if let Some((s, e)) = parse_episode(&row.title) {
            let season = s.or(season_dir).unwrap_or(1);
            return Target {
                query: format!("{show} S{season:02}E{e:02}"),
                year: None,
                episode: Some((season, e)),
            };
        }
        let (name, year) = clean_title(&row.title);
        return Target { query: format!("{show} {name}").trim().to_string(), year, episode: None };
    }
    let (name, year) = clean_title(&row.title);
    Target { query: name, year, episode: None }
}

/// 文本里是否含目标集号（S03E05 / s3e5 / E05 等写法）
fn has_episode(text: &str, season: u32, ep: u32) -> bool {
    let u = text.to_ascii_uppercase();
    let forms = [
        format!("S{season:02}E{ep:02}"),
        format!("S{season}E{ep:02}"),
        format!("S{season:02}E{ep}"),
        format!("S{season}E{ep}"),
    ];
    if forms.iter().any(|f| contains_token(&u, f)) {
        return true;
    }
    // 只有集号（无季号）的写法：仅第 1 季认，且前面不能紧挨数字（排除 S02E01 里的 E01）
    season == 1 && [format!("E{ep:02}"), format!("EP{ep:02}")].iter().any(|f| contains_bare(&u, f))
}

fn contains_bare(hay: &str, needle: &str) -> bool {
    let b = hay.as_bytes();
    let mut from = 0;
    while let Some(i) = hay[from..].find(needle) {
        let start = from + i;
        let end = start + needle.len();
        let prev_digit = start > 0 && b[start - 1].is_ascii_digit();
        let next_digit = b.get(end).is_some_and(|c| c.is_ascii_digit());
        if !prev_digit && !next_digit {
            return true;
        }
        from = start + 1;
    }
    false
}

/// needle 后面不能紧跟数字（避免 E1 命中 E12）
fn contains_token(hay: &str, needle: &str) -> bool {
    let mut from = 0;
    while let Some(i) = hay[from..].find(needle) {
        let end = from + i + needle.len();
        if !hay.as_bytes().get(end).is_some_and(|c| c.is_ascii_digit()) {
            return true;
        }
        from += i + 1;
    }
    false
}

/// 文本里出现的年份（1900–2099，前后不紧挨数字）
fn years_in(text: &str) -> Vec<u32> {
    let b = text.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i + 4 <= b.len() {
        let w = &b[i..i + 4];
        let prev_digit = i > 0 && b[i - 1].is_ascii_digit();
        let next_digit = b.get(i + 4).is_some_and(|c| c.is_ascii_digit());
        if w.iter().all(|c| c.is_ascii_digit()) && !prev_digit && !next_digit && (w[0..2] == *b"19" || w[0..2] == *b"20") {
            if let Ok(y) = std::str::from_utf8(w).unwrap_or("").parse::<u32>() {
                out.push(y);
            }
            i += 4;
        } else {
            i += 1;
        }
    }
    out
}

/// 片名后面紧跟数字（第一滴血5、敢死队3）：是续集，不是要找的这部。
/// 只要有一处片名后面不是数字就不算（如「第一滴血.Rambo.1982」）。
fn is_sequel(text: &str, name: &str) -> bool {
    if name.is_empty() {
        return false;
    }
    let mut seen = false;
    let mut from = 0;
    while let Some(i) = text[from..].find(name) {
        let end = from + i + name.len();
        seen = true;
        if !text[end..].chars().next().is_some_and(|c| c.is_ascii_digit()) {
            return false;
        }
        from = end;
    }
    seen
}

/// 整季包标记：S03 后面既不是数字也不是 E（S03E04 是单集，不算）
fn is_season_pack(upper: &str, season: u32) -> bool {
    let needle = format!("S{season:02}");
    let b = upper.as_bytes();
    let mut from = 0;
    while let Some(i) = upper[from..].find(&needle) {
        let end = from + i + needle.len();
        match b.get(end) {
            Some(c) if c.is_ascii_digit() || *c == b'E' => {}
            _ => return true,
        }
        from += i + 1;
    }
    false
}

fn is_ass_name(name: &str) -> bool {
    let l = name.to_ascii_lowercase();
    l.ends_with(".ass")
}

/// 同一个包里有多个 .ass 时的偏好：简英双语 > 简体 > 其它，繁体靠后。
fn file_pref(name: &str) -> i32 {
    let l = name.to_lowercase();
    let mut s = 0;
    let chs = l.contains("chs") || l.contains("简") || l.contains(".sc") || l.contains("gb");
    let eng = l.contains("eng") || l.contains("英") || l.contains("双语");
    let cht = l.contains("cht") || l.contains("繁") || l.contains(".tc") || l.contains("big5");
    if chs { s += 2; }
    if eng { s += 2; }
    if cht && !chs { s -= 3; }
    s
}

// ---------------- API ----------------

struct Client {
    agent: ureq::Agent,
    token: String,
    limiter: Limiter,
}

#[derive(Debug)]
enum ApiError {
    /// 网络/解析/服务端错误，计入连续失败
    Api(String),
    /// Key 无效等：再试也没用，整批立即中止
    Fatal(String),
}

impl Client {
    fn new(token: String) -> Self {
        let agent = ureq::AgentBuilder::new()
            .timeout_connect(Duration::from_secs(15))
            .timeout(Duration::from_secs(30))
            .user_agent(USER_AGENT)
            .build();
        Self { agent, token, limiter: Limiter::new() }
    }

    fn call(&mut self, path: &str, params: &[(&str, String)]) -> Result<serde_json::Value, ApiError> {
        let mut query = format!("token={}", urlencoding::encode(&self.token));
        for (k, v) in params {
            query.push_str(&format!("&{k}={}", urlencoding::encode(v)));
        }
        let mut last_err = String::new();
        let mut body = None;
        for attempt in 0..NET_ATTEMPTS {
            if attempt > 0 {
                std::thread::sleep(Duration::from_secs(3 * attempt as u64));
            }
            self.limiter.acquire();
            let url = format!("{}/{path}?{query}", API_BASES[attempt % API_BASES.len()]);
            match self.agent.get(&url).call() {
                Ok(resp) => match resp.into_string() {
                    Ok(b) => {
                        body = Some(b);
                        break;
                    }
                    Err(e) => last_err = format!("读取响应失败: {e}"),
                },
                // 服务端明确回了 HTTP 错误码：重试也一样，不再试
                Err(ureq::Error::Status(code, _)) => {
                    return Err(ApiError::Api(format!("射手网 HTTP {code}")));
                }
                Err(ureq::Error::Transport(t)) => last_err = describe_transport(&t),
            }
        }
        let Some(body) = body else {
            return Err(ApiError::Api(format!("连不上射手网（已重试 {NET_ATTEMPTS} 次）: {last_err}")));
        };
        let json: serde_json::Value =
            serde_json::from_str(&body).map_err(|e| ApiError::Api(format!("响应不是 JSON: {e}")))?;
        let status = json["status"].as_i64().unwrap_or(-1);
        if status == 20001 {
            return Err(ApiError::Fatal("射手网 API Key 无效，请到设置中检查".into()));
        }
        if status != 0 {
            let msg = json["errmsg"].as_str().unwrap_or("未知错误");
            return Err(ApiError::Api(format!("射手网返回错误 {status}: {msg}")));
        }
        Ok(json)
    }

    fn search(&mut self, q: &str) -> Result<Vec<serde_json::Value>, ApiError> {
        let json = self.call("sub/search", &[("q", q.to_string()), ("cnt", "15".into()), ("pos", "0".into())])?;
        // 无结果时 subs 是 {} 而不是数组
        Ok(json["sub"]["subs"].as_array().map(|a| a.iter().map(normalize_candidate).collect()).unwrap_or_default())
    }

    fn detail(&mut self, id: i64) -> Result<Option<serde_json::Value>, ApiError> {
        let json = self.call("sub/detail", &[("id", id.to_string())])?;
        Ok(json["sub"]["subs"].as_array().and_then(|a| a.first()).cloned())
    }

    /// 下载字幕文件（不占 API 配额）；网络错误时再试一次
    fn download(&self, url: &str) -> Result<Vec<u8>, String> {
        let resp = match self.agent.get(url).call() {
            Err(ureq::Error::Transport(_)) => {
                std::thread::sleep(Duration::from_secs(3));
                self.agent.get(url).call()
            }
            r => r,
        }
        .map_err(|e| format!("下载失败: {e}"))?;
        let mut buf = Vec::new();
        resp.into_reader()
            .take(MAX_DOWNLOAD_BYTES)
            .read_to_end(&mut buf)
            .map_err(|e| format!("下载失败: {e}"))?;
        if buf.is_empty() {
            return Err("下载到的文件为空".into());
        }
        Ok(buf)
    }
}

/// 网络错误的说明：不带 URL（URL 里有 API Key，不能出现在界面和日志里）。
fn describe_transport(t: &ureq::Transport) -> String {
    let mut s = t.kind().to_string();
    if let Some(m) = t.message() {
        s.push_str(": ");
        s.push_str(m);
    }
    if let Some(src) = std::error::Error::source(t) {
        s.push_str(": ");
        s.push_str(&src.to_string());
    }
    s
}

/// 搜索结果有两种格式混在一起返回：文档里的（id / subtype / native_name / lang.langlist），
/// 和另一种内部格式（fileid / m_subtype / m_title / m_langn …，其中 subtype 是数字编号）。
/// 统一成文档格式，后面的打分只认这一种。
fn normalize_candidate(c: &serde_json::Value) -> serde_json::Value {
    use serde_json::{json, Value};
    if c["id"].is_i64() || c["id"].is_u64() {
        return c.clone();
    }
    let id = c["fileid"]
        .as_str()
        .and_then(|s| s.parse::<i64>().ok())
        .or_else(|| c["fileid"].as_i64());
    let str_of = |k: &str| c[k].as_str().unwrap_or("").to_string();
    let subtype = c["m_subtype"].as_str().map(str::to_string).unwrap_or_default();
    let native = [str_of("m_title"), str_of("video_chinese_name"), str_of("sub_name")]
        .into_iter()
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join(" ");
    let videoname = [str_of("videoname"), str_of("m_videoname")]
        .into_iter()
        .filter(|s| !s.is_empty() && s != "不知道")
        .collect::<Vec<_>>()
        .join(" ");
    let langs: Vec<String> = c["m_langn"]
        .as_array()
        .map(|a| a.iter().filter_map(|v| v.as_str().map(str::to_string)).collect())
        .unwrap_or_default();
    let has = |k: &str| langs.iter().any(|l| l == k) || c["m_extras"][k].as_str() == Some("1");
    let vote = c["score"].as_str().and_then(|s| s.parse::<i64>().ok()).unwrap_or(0);
    json!({
        "id": id.map(Value::from).unwrap_or(Value::Null),
        "subtype": subtype,
        "native_name": native,
        "videoname": videoname,
        "vote_score": vote,
        "lang": { "langlist": {
            "langchs": has("langchs"), "langeng": has("langeng"),
            "langdou": has("langdou"), "langcht": has("langcht"),
        }},
    })
}

/// 搜索结果打分；返回 None 表示不要（非 ASS、剧集集号对不上）。
fn score_candidate(c: &serde_json::Value, t: &Target) -> Option<i64> {
    let subtype = c["subtype"].as_str().unwrap_or("").to_ascii_lowercase();
    // 只要 ASS：subtype 明确不含 ass/ssa 的直接排除（空的放行，详情里再按文件名把关）
    if !subtype.is_empty() && !subtype.contains("ass") && !subtype.contains("ssa") {
        return None;
    }
    let text = format!(
        "{} {}",
        c["videoname"].as_str().unwrap_or(""),
        c["native_name"].as_str().unwrap_or("")
    );
    let mut s: i64 = 0;
    if let Some((season, ep)) = t.episode {
        if has_episode(&text, season, ep) {
            s += 10;
        } else {
            // 整季打包：含本季标记且不含别的集号时也可能在包里，分数低一些
            let u = text.to_ascii_uppercase();
            let season_tag = is_season_pack(&u, season) || text.contains(&format!("第{season}季"));
            if !season_tag {
                return None;
            }
        }
    }
    if t.episode.is_none() {
        if is_sequel(&text, &t.query) {
            return None;
        }
        if let Some(y) = t.year {
            let years = years_in(&text);
            if years.contains(&y) {
                s += 5;
            } else if !years.is_empty() {
                return None; // 标着别的年份：多半是同系列的另一部
            }
        }
    }
    let ll = &c["lang"]["langlist"];
    let chs = ll["langchs"].as_bool().unwrap_or(false);
    let eng = ll["langeng"].as_bool().unwrap_or(false);
    let dou = ll["langdou"].as_bool().unwrap_or(false);
    if chs && eng || dou { s += 4; } else if chs { s += 2; }
    if !chs && ll["langcht"].as_bool().unwrap_or(false) { s -= 2; }
    if subtype.contains("ass") { s += 1; }
    // 评分作为同分时的次序
    s = s * 1000 + c["vote_score"].as_i64().unwrap_or(0).clamp(0, 999);
    Some(s)
}

/// 从若干 (文件名, 内容) 里挑最合适的 .ass：剧集多文件时按集号过滤，再按简英偏好排序。
fn choose_ass<T>(mut items: Vec<(String, T)>, t: &Target) -> Option<T> {
    items.retain(|(n, _)| is_ass_name(n));
    if let Some((s, e)) = t.episode {
        // 单集字幕包里文件名常不带集号；多个文件时才要求集号匹配
        if items.len() > 1 {
            items.retain(|(n, _)| has_episode(n, s, e));
        }
    }
    items.sort_by_key(|(n, _)| std::cmp::Reverse(file_pref(n)));
    items.into_iter().next().map(|(_, v)| v)
}

/// 射手网出错时会把下载跳转到一个 XML/HTML 错误页；有的还是 200，按内容再把一道关。
fn looks_like_error_page(bytes: &[u8]) -> bool {
    let head = String::from_utf8_lossy(&bytes[..bytes.len().min(64)]).to_ascii_lowercase();
    let head = head.trim_start_matches('\u{feff}').trim_start();
    head.starts_with("<?xml") || head.starts_with("<!doctype") || head.starts_with("<html")
}

/// 从详情里取 .ass：
/// 1) filelist 里单个 .ass 的直链（onthefly，服务端现解压）——经常 493「服务器抽风」，失败不算数；
/// 2) 下载整个字幕包（url），在本地按实际格式（zip / rar / 7z / 纯 .ass）解出 .ass。
fn fetch_ass(client: &Client, detail: &serde_json::Value, t: &Target) -> Result<Option<Vec<u8>>, String> {
    if let Some(list) = detail["filelist"].as_array() {
        let files: Vec<(String, String)> = list
            .iter()
            .filter_map(|f| Some((f["f"].as_str()?.to_string(), f["url"].as_str()?.to_string())))
            .collect();
        let has_any = !files.is_empty();
        match choose_ass(files, t) {
            Some(url) => {
                if let Ok(bytes) = client.download(&url) {
                    if !looks_like_error_page(&bytes) {
                        return Ok(Some(bytes));
                    }
                }
                // 直链失败：往下走整包下载
            }
            None if has_any => return Ok(None), // 有文件清单但没有（对得上集号的）.ass
            None => {}
        }
    }
    let url = match detail["url"].as_str() {
        Some(u) if !u.is_empty() => u,
        _ => return Ok(None),
    };
    let filename = detail["filename"].as_str().unwrap_or("").to_string();
    let bytes = client.download(url)?;
    if looks_like_error_page(&bytes) {
        return Err("射手网返回错误页，下载失败".into());
    }
    extract_ass(&bytes, &filename, t)
}

/// 按文件头判断格式（射手网的 .zip 常常其实是 rar），解出最合适的 .ass。
fn extract_ass(bytes: &[u8], filename: &str, t: &Target) -> Result<Option<Vec<u8>>, String> {
    let entries = if bytes.starts_with(b"PK\x03\x04") {
        zip_entries(bytes)?
    } else if bytes.starts_with(b"Rar!\x1a\x07") {
        rar_entries(bytes)?
    } else if bytes.starts_with(b"7z\xbc\xaf\x27\x1c") {
        sevenz_entries(bytes)?
    } else if is_ass_name(filename) {
        vec![(filename.to_string(), bytes.to_vec())]
    } else {
        return Ok(None);
    };
    Ok(choose_ass(entries, t).filter(|b| !b.is_empty()))
}

fn zip_entries(bytes: &[u8]) -> Result<Vec<(String, Vec<u8>)>, String> {
    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(bytes)).map_err(|e| format!("zip 解压失败: {e}"))?;
    let mut out = Vec::new();
    for i in 0..zip.len() {
        let Ok(mut f) = zip.by_index(i) else { continue };
        let name = f.name().to_string();
        if !is_ass_name(&name) {
            continue;
        }
        let mut buf = Vec::new();
        if f.read_to_end(&mut buf).is_ok() {
            out.push((name, buf));
        }
    }
    Ok(out)
}

/// 临时文件名前缀：只有 rar 需要落盘（unrar 只能读文件），zip/7z 都在内存里解。
const TMP_PREFIX: &str = "collector_subfill_";

/// 临时文件守卫：离开作用域（含出错、panic 展开）即删除。
struct TempFile(PathBuf);
impl Drop for TempFile {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

/// 清掉上次异常退出（如被强杀）遗留的临时包。只删本模块前缀、且是文件的条目。
fn sweep_stale_temp() {
    let Ok(rd) = std::fs::read_dir(std::env::temp_dir()) else { return };
    for e in rd.flatten() {
        let name = e.file_name();
        let name = name.to_string_lossy();
        if name.starts_with(TMP_PREFIX) && name.ends_with(".rar") && e.path().is_file() {
            let _ = std::fs::remove_file(e.path());
        }
    }
}

/// unrar 只能读磁盘文件：先落到临时目录，读完即删。下载的压缩包本身从不落盘。
fn rar_entries(bytes: &[u8]) -> Result<Vec<(String, Vec<u8>)>, String> {
    use std::sync::atomic::{AtomicUsize, Ordering};
    static SEQ: AtomicUsize = AtomicUsize::new(0);
    let tmp = TempFile(std::env::temp_dir().join(format!(
        "{TMP_PREFIX}{}_{}.rar",
        std::process::id(),
        SEQ.fetch_add(1, Ordering::Relaxed)
    )));
    std::fs::write(&tmp.0, bytes).map_err(|e| format!("写临时文件失败: {e}"))?;
    let tmp_path = tmp.0.clone();
    (|| -> Result<Vec<(String, Vec<u8>)>, String> {
        let mut out = Vec::new();
        let mut archive = unrar::Archive::new(&tmp_path)
            .open_for_processing()
            .map_err(|e| format!("rar 解压失败: {e}"))?;
        while let Some(header) = archive.read_header().map_err(|e| format!("rar 解压失败: {e}"))? {
            let name = header.entry().filename.to_string_lossy().into_owned();
            archive = if header.entry().is_file() && is_ass_name(&name) {
                let (data, rest) = header.read().map_err(|e| format!("rar 解压失败: {e}"))?;
                out.push((name, data));
                rest
            } else {
                header.skip().map_err(|e| format!("rar 解压失败: {e}"))?
            };
        }
        Ok(out)
    })()
    // tmp 在此 drop，临时包随即删除
}

fn sevenz_entries(bytes: &[u8]) -> Result<Vec<(String, Vec<u8>)>, String> {
    let mut reader = sevenz_rust2::ArchiveReader::new(std::io::Cursor::new(bytes), sevenz_rust2::Password::empty())
        .map_err(|e| format!("7z 解压失败: {e}"))?;
    let mut out = Vec::new();
    reader
        .for_each_entries(|entry, r| {
            if !entry.is_directory() && is_ass_name(entry.name()) {
                let mut buf = Vec::new();
                r.read_to_end(&mut buf)?;
                out.push((entry.name().to_string(), buf));
            }
            Ok(true)
        })
        .map_err(|e| format!("7z 解压失败: {e}"))?;
    Ok(out)
}

enum Outcome {
    Downloaded(Vec<u8>),
    NotFound,
}

/// 为一个视频找并下载 ASS。API 错误向上抛（计入连续失败），其它问题算未找到/失败。
fn find_one(client: &mut Client, t: &Target) -> Result<Result<Outcome, String>, ApiError> {
    let results = client.search(&t.query)?;
    let mut scored: Vec<(i64, &serde_json::Value)> = results
        .iter()
        .filter_map(|c| score_candidate(c, t).map(|s| (s, c)))
        .collect();
    scored.sort_by_key(|(s, _)| std::cmp::Reverse(*s));
    let mut last_err = None;
    for (_, c) in scored.into_iter().take(MAX_DETAILS) {
        let Some(id) = c["id"].as_i64() else { continue };
        let Some(detail) = client.detail(id)? else { continue };
        match fetch_ass(client, &detail, t) {
            Ok(Some(bytes)) => return Ok(Ok(Outcome::Downloaded(bytes))),
            Ok(None) => {}
            Err(e) => last_err = Some(e),
        }
    }
    Ok(match last_err {
        Some(e) => Err(e),
        None => Ok(Outcome::NotFound),
    })
}

/// 进度回调参数：(已处理, 待补总数, 当前文件)
pub fn run_fill(
    in_dir: &Path,
    rows: &[MediaRow],
    token: &str,
    mut progress: impl FnMut(usize, usize, &str),
) -> AppResult<FillReport> {
    if token.trim().is_empty() {
        return Err(AppError::Invalid("请先在设置中填写射手网 API Key".into()));
    }
    let mut report = FillReport { total: rows.len(), ..Default::default() };
    let missing: Vec<&MediaRow> = rows.iter().filter(|r| !subtitle_abs(in_dir, r).is_file()).collect();
    report.existing = rows.len() - missing.len();
    let total = missing.len();
    progress(0, total, "");

    sweep_stale_temp();
    let mut client = Client::new(token.trim().to_string());
    let mut consecutive_api_errors = 0;
    for (i, row) in missing.iter().enumerate() {
        let rel = subtitle_rel(row);
        progress(i, total, &rel);
        let target = build_target(row);
        match find_one(&mut client, &target) {
            Ok(Ok(Outcome::Downloaded(bytes))) => {
                consecutive_api_errors = 0;
                let dest = subtitle_abs(in_dir, row);
                let write = dest
                    .parent()
                    .map(std::fs::create_dir_all)
                    .unwrap_or(Ok(()))
                    .and_then(|_| std::fs::write(&dest, &bytes));
                match write {
                    Ok(()) => report.downloaded.push(rel),
                    Err(e) => report.failed.push(FillFailure { file: rel, reason: format!("写入失败: {e}") }),
                }
            }
            Ok(Ok(Outcome::NotFound)) => {
                consecutive_api_errors = 0;
                report.not_found.push(rel);
            }
            Ok(Err(reason)) => {
                consecutive_api_errors = 0;
                report.failed.push(FillFailure { file: rel, reason });
            }
            Err(ApiError::Fatal(reason)) => {
                report.aborted = Some(reason);
                progress(i + 1, total, "");
                return Ok(report);
            }
            Err(ApiError::Api(reason)) => {
                consecutive_api_errors += 1;
                report.failed.push(FillFailure { file: rel, reason: reason.clone() });
                if consecutive_api_errors >= MAX_CONSECUTIVE_API_ERRORS {
                    report.aborted = Some(format!("连续 {MAX_CONSECUTIVE_API_ERRORS} 次请求失败，已停止：{reason}"));
                    progress(i + 1, total, "");
                    return Ok(report);
                }
            }
        }
        progress(i + 1, total, "");
    }
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(c: &str, p: &str, t: &str) -> MediaRow {
        MediaRow { category: c.into(), category_path: p.into(), title: t.into() }
    }

    #[test]
    fn rel_path_mirrors_library() {
        assert_eq!(subtitle_rel(&row("电影", "动作题材/敢死队", "[2012].敢死队2")), "电影/动作题材/敢死队/[2012].敢死队2.ass");
        assert_eq!(subtitle_rel(&row("电影", "", "沙丘")), "电影/沙丘.ass");
    }

    #[test]
    fn targets() {
        let t = build_target(&row("电影", "动作题材/敢死队", "[2012].敢死队2"));
        assert_eq!(t.query, "敢死队2");
        assert_eq!(t.year, Some(2012));
        let t = build_target(&row("剧集", "英剧/黑镜/第3季", "S03E05.人与武器的对抗.Men.Against.Fire"));
        assert_eq!(t.query, "黑镜 S03E05");
        assert_eq!(t.episode, Some((3, 5)));
        let t = build_target(&row("剧集", "日剧/怨屋本铺", "E01.怨屋本铺"));
        assert_eq!(t.query, "怨屋本铺 S01E01");
        assert_eq!(t.episode, Some((1, 1)));
    }

    #[test]
    fn episode_match() {
        assert!(has_episode("Black.Mirror.S03E05.720p", 3, 5));
        assert!(!has_episode("Black.Mirror.S03E15.720p", 3, 5));
        assert!(!has_episode("Black.Mirror.S03E05", 3, 6));
        assert!(has_episode("xxx.E01.chs", 1, 1));
        assert!(!has_episode("xxx.E01.chs", 2, 1));
        assert!(!has_episode("Show.S02E01", 1, 1));
    }

    #[test]
    fn candidate_filter() {
        let t = Target { query: "x".into(), year: Some(2012), episode: None };
        let srt = serde_json::json!({"subtype":"SRT","id":1});
        assert!(score_candidate(&srt, &t).is_none());
        let a = serde_json::json!({"subtype":"ASS","id":1,"videoname":"The.Expendables.2.2012","lang":{"langlist":{"langchs":true,"langeng":true}}});
        let b = serde_json::json!({"subtype":"ASS","id":2,"videoname":"x","lang":{"langlist":{"langcht":true}}});
        assert!(score_candidate(&a, &t).unwrap() > score_candidate(&b, &t).unwrap());
        let te = Target { query: "x".into(), year: None, episode: Some((3, 5)) };
        let wrong = serde_json::json!({"subtype":"ASS","id":3,"videoname":"Black.Mirror.S03E04"});
        assert!(score_candidate(&wrong, &te).is_none());
        let pack = serde_json::json!({"subtype":"ASS","id":4,"videoname":"Black.Mirror.S03.Complete"});
        assert!(score_candidate(&pack, &te).is_some());
    }

    #[test]
    fn rejects_sequels_and_wrong_years() {
        let t = Target { query: "第一滴血".into(), year: Some(1982), episode: None };
        let five = serde_json::json!({"subtype":"SSA","id":1,"native_name":"第一滴血5.最后的血.Rambo.Last.Blood.2019"});
        let four = serde_json::json!({"subtype":"SSA","id":2,"native_name":"第一滴血4.rambo.2008"});
        let one = serde_json::json!({"subtype":"SSA","id":3,"native_name":"第一滴血.Rambo.First.Blood.1982.2160p"});
        let noyear = serde_json::json!({"subtype":"SSA","id":4,"native_name":"第一滴血 简英"});
        assert!(score_candidate(&five, &t).is_none());
        assert!(score_candidate(&four, &t).is_none());
        assert!(score_candidate(&noyear, &t).is_some());
        assert!(score_candidate(&one, &t).unwrap() > score_candidate(&noyear, &t).unwrap());
        assert_eq!(years_in("Rambo.2160p.1982.x265"), vec![1982]);
    }

    #[test]
    fn normalizes_internal_format() {
        let raw = serde_json::json!({"fileid":"679990","m_subtype":"SSA","subtype":"3",
            "m_title":"第一滴血.Rambo.First.Blood.1982.2160p","m_langn":["langeng","langchs","langdou"],
            "videoname":"第一滴血.Rambo.First.Blood.1982","score":"0"});
        let c = normalize_candidate(&raw);
        assert_eq!(c["id"].as_i64(), Some(679990));
        let t = Target { query: "第一滴血".into(), year: Some(1982), episode: None };
        assert!(score_candidate(&c, &t).is_some());
        let srt = normalize_candidate(&serde_json::json!({"fileid":"1","m_subtype":"Subrip(srt)","subtype":"2"}));
        assert!(score_candidate(&srt, &t).is_none());
    }

    #[test]
    fn transport_error_hides_token() {
        let agent = ureq::AgentBuilder::new().timeout(Duration::from_secs(5)).build();
        match agent.get("http://127.0.0.1:1/v1/sub/search?token=SECRET123&q=x").call() {
            Err(ureq::Error::Transport(t)) => {
                let d = describe_transport(&t);
                assert!(!d.contains("SECRET123"), "{d}");
                assert!(!d.is_empty());
            }
            other => panic!("expected transport error, got {other:?}"),
        }
    }

    #[test]
    fn detects_error_page() {
        assert!(looks_like_error_page(b"<?xml version=\"1.0\"?><page/>"));
        assert!(!looks_like_error_page(b"[Script Info]
Title: x"));
    }

    #[test]
    fn extracts_from_zip_by_magic() {
        let mut buf = std::io::Cursor::new(Vec::new());
        {
            let mut w = zip::ZipWriter::new(&mut buf);
            let o = zip::write::SimpleFileOptions::default();
            w.start_file("a.cht.ass", o).unwrap();
            std::io::Write::write_all(&mut w, b"cht").unwrap();
            w.start_file("a.chs&eng.ass", o).unwrap();
            std::io::Write::write_all(&mut w, b"chs").unwrap();
            w.start_file("a.srt", o).unwrap();
            std::io::Write::write_all(&mut w, b"srt").unwrap();
            w.finish().unwrap();
        }
        let t = Target { query: "x".into(), year: None, episode: None };
        // 扩展名写的是 .rar 也按文件头识别
        assert_eq!(extract_ass(buf.get_ref(), "x.rar", &t).unwrap(), Some(b"chs".to_vec()));
    }

    /// 真实 rar 包：SUBFILL_RAR=<路径> cargo test -- --ignored rar_real
    #[test]
    #[ignore]
    fn rar_real() {
        let p = std::env::var("SUBFILL_RAR").unwrap();
        let bytes = std::fs::read(p).unwrap();
        let t = Target { query: "x".into(), year: None, episode: None };
        let got = extract_ass(&bytes, "x.zip", &t).unwrap().expect("no ass");
        assert!(String::from_utf8_lossy(&got).contains("[Script Info]"));
        let left = std::fs::read_dir(std::env::temp_dir()).unwrap().flatten()
            .filter(|e| e.file_name().to_string_lossy().starts_with(&format!("{TMP_PREFIX}{}_", std::process::id())))
            .count();
        assert_eq!(left, 0, "临时 rar 未删除");
    }

    /// 联网实测：SUBFILL_TOKEN=<key> SUBFILL_OUT=<目录> cargo test -- --ignored live_fill
    #[test]
    #[ignore]
    fn live_fill() {
        let token = std::env::var("SUBFILL_TOKEN").unwrap();
        let out = PathBuf::from(std::env::var("SUBFILL_OUT").unwrap());
        let rows = vec![
            row("电影", "动作题材/第一滴血", "[1982].第一滴血"),
            row("剧集", "英剧/黑镜/第3季", "S03E05.人与武器的对抗.Men.Against.Fire"),
        ];
        let r = run_fill(&out, &rows, &token, |d, t, c| eprintln!("{d}/{t} {c}")).unwrap();
        eprintln!("{r:#?}");
        assert!(r.aborted.is_none());
        for f in &r.downloaded {
            let text = crate::normalize::encoding::read_subtitle(&out.join(f)).expect("decode");
            assert!(text.contains("[Script Info]"), "{f}");
        }
    }

    #[test]
    fn prefers_chs_eng_file() {
        assert!(file_pref("a.chs&eng.ass") > file_pref("a.cht.ass"));
        assert!(is_ass_name("X.ASS"));
        assert!(!is_ass_name("x.srt"));
    }
}
