//! 从目录结构/标题解析 TMDB 搜索元数据（纯函数）。

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MediaKind {
    Movie,
    Tv,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MediaQuery {
    pub name: String,
    pub kind: MediaKind,
    pub season: Option<u32>,
    pub year: Option<u32>,
    /// 动漫标记：为 true 时编排层先搜 movie（剧场版），未命中再 fallback 搜 tv。
    pub is_anime: bool,
    /// 副标题降级用的主名：name 含「：/:」时为冒号前主名，供完整名未命中时再搜。否则 None。
    pub alt_name: Option<String>,
}

/// 解析季号：匹配「第N季」开头，其后可跟副标题（如「第1季：血与沙」）。非季目录返回 None。
fn parse_season(seg: &str) -> Option<u32> {
    let rest = seg.strip_prefix('第')?;
    let idx = rest.find('季')?;
    let num = &rest[..idx];
    if num.is_empty() || !num.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    let trimmed = num.trim_start_matches('0');
    if trimmed.is_empty() {
        num.parse::<u32>().ok()
    } else {
        trimmed.parse::<u32>().ok()
    }
}

/// 从 title 剥离年份前缀，返回 (干净片名, 年份)。支持两种前缀：
/// - 「[YYYY].」或「[YYYY]」：如 [1993].侏罗纪公园 → ("侏罗纪公园", Some(1993))
/// - 裸「YYYY.」：如 2006.寂静岭 → ("寂静岭", Some(2006))
/// 保留片名中间的所有标点（. ： ～ 等）。无年份前缀则原样返回、year=None。
pub fn clean_title(title: &str) -> (String, Option<u32>) {
    // 形式一：[YYYY] 前缀
    if let Some(rest) = title.strip_prefix('[') {
        if let Some(close) = rest.find(']') {
            let inner = &rest[..close];
            if inner.len() == 4 && inner.chars().all(|c| c.is_ascii_digit()) {
                let year = inner.parse::<u32>().ok();
                let after = rest[close + 1..].strip_prefix('.').unwrap_or(&rest[close + 1..]);
                return (after.trim().to_string(), year);
            }
        }
    }
    // 形式二：裸 YYYY. 前缀（按字节安全：前 5 个字节须为 4 位 ASCII 数字 + '.'）
    let t = title.trim();
    let b = t.as_bytes();
    if b.len() > 5 && b[..4].iter().all(|c| c.is_ascii_digit()) && b[4] == b'.' {
        let year = t[..4].parse::<u32>().ok();
        return (t[5..].trim().to_string(), year);
    }
    (t.to_string(), None)
}

/// 若 name 含中文/英文冒号，返回冒号前主名（用于副标题降级搜索）；否则 None。
/// 例：指环王3：国王归来 → Some("指环王3")；星球大战 → None。
fn subtitle_main(name: &str) -> Option<String> {
    let idx = name.find('：').or_else(|| name.find(':'))?;
    let main = name[..idx].trim();
    if main.is_empty() || main == name { None } else { Some(main.to_string()) }
}

/// 从片名生成 TMDB 搜索候选集，供优化建议探索。
/// 按多标点（！!。.·：: 及空格）切分词段，生成：完整名、各单词段、
/// 前缀累加组合、去首段的后缀组合；去重去空、过滤单字，按长度降序返回
/// （越长越可能是完整片名，优先命中准确条目而非系列泛名）。
pub fn suggest_candidates(name: &str) -> Vec<String> {
    let seps = ['！', '!', '。', '.', '·', '：', ':', ' ', '　'];
    let segs: Vec<String> = name
        .split(|c| seps.contains(&c))
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();

    let mut out: Vec<String> = Vec::new();
    let mut push = |s: String, out: &mut Vec<String>| {
        let s = s.trim().to_string();
        if s.chars().count() >= 2 && !out.contains(&s) {
            out.push(s);
        }
    };
    // 完整名
    push(name.trim().to_string(), &mut out);
    // 各单词段
    for s in &segs {
        push(s.clone(), &mut out);
    }
    // 前缀累加组合（seg[0..k] 拼接）
    for k in 1..=segs.len() {
        push(segs[..k].concat(), &mut out);
    }
    // 去首段的后缀组合（seg[1..] 拼接）
    if segs.len() >= 2 {
        push(segs[1..].concat(), &mut out);
    }
    // 按字符长度降序（越长越可能是完整片名）
    out.sort_by(|a, b| b.chars().count().cmp(&a.chars().count()));
    out
}

/// 从 category / category_path / title 解析出 TMDB 搜索元数据。
/// - 电影：用条目 title（剥离年份前缀）作片名，Movie，带 year；含副标题时 alt_name=主名。
/// - 动漫：同电影取名，kind=Movie（先搜剧场版）+ is_anime=true（编排层未命中 fallback tv）。
/// - 剧集：若末段是「第N季[：副标题]」，剧名取倒数第二段、season=N；否则末段为剧名。
pub fn parse_query(category: &str, category_path: &str, title: &str) -> MediaQuery {
    let segs: Vec<&str> = category_path.split('/').filter(|s| !s.is_empty()).collect();
    let last = segs.last().copied().unwrap_or(title);

    match category {
        "剧集" => {
            if let Some(season) = parse_season(last) {
                let name = segs
                    .get(segs.len().saturating_sub(2))
                    .copied()
                    .unwrap_or(last)
                    .to_string();
                MediaQuery { name, kind: MediaKind::Tv, season: Some(season), year: None, is_anime: false, alt_name: None }
            } else {
                MediaQuery { name: last.to_string(), kind: MediaKind::Tv, season: None, year: None, is_anime: false, alt_name: None }
            }
        }
        "动漫" => {
            let (name, year) = clean_title(title);
            let alt_name = subtitle_main(&name);
            MediaQuery { name, kind: MediaKind::Movie, season: None, year, is_anime: true, alt_name }
        }
        _ => {
            let (name, year) = clean_title(title);
            let alt_name = subtitle_main(&name);
            MediaQuery { name, kind: MediaKind::Movie, season: None, year, is_anime: false, alt_name }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clean_title_strips_year_prefix() {
        assert_eq!(clean_title("[1993].侏罗纪公园"), ("侏罗纪公园".to_string(), Some(1993)));
        assert_eq!(
            clean_title("[2006].致工藤新一的挑战书～离别前的序章"),
            ("致工藤新一的挑战书～离别前的序章".to_string(), Some(2006))
        );
        // 无前缀原样返回
        assert_eq!(clean_title("海贼王"), ("海贼王".to_string(), None));
        // 非四位数字方括号不当作年份
        assert_eq!(clean_title("[HD].某片"), ("[HD].某片".to_string(), None));
    }

    #[test]
    fn movie_uses_title_with_year() {
        let q = parse_query("电影", "电影/科幻/星球大战", "[1977].星球大战");
        assert_eq!(q.name, "星球大战");
        assert!(matches!(q.kind, MediaKind::Movie));
        assert_eq!(q.season, None);
        assert_eq!(q.year, Some(1977));
        assert!(!q.is_anime);
    }

    #[test]
    fn anime_searches_movie_first_with_year() {
        let q = parse_query("动漫", "动漫/日本/北斗神拳", "[2007].尤莉亚传");
        assert_eq!(q.name, "尤莉亚传");
        assert!(matches!(q.kind, MediaKind::Movie), "动漫先按 movie 搜（剧场版）");
        assert!(q.is_anime, "标记为动漫，编排层会 fallback 到 tv");
        assert_eq!(q.season, None);
        assert_eq!(q.year, Some(2007));
    }

    #[test]
    fn tv_normal_season() {
        let q = parse_query("剧集", "剧集/英剧/神探夏洛克/第2季", "S02E01");
        assert_eq!(q.name, "神探夏洛克");
        assert!(matches!(q.kind, MediaKind::Tv));
        assert_eq!(q.season, Some(2));
        assert_eq!(q.year, None);
        assert!(!q.is_anime);
    }

    #[test]
    fn tv_zero_padded_season() {
        let q = parse_query("剧集", "剧集/美剧/行尸走肉/第07季", "x");
        assert_eq!(q.name, "行尸走肉");
        assert_eq!(q.season, Some(7));
    }

    #[test]
    fn tv_no_season_layer() {
        let q = parse_query("剧集", "剧集/美剧/权力的游戏", "x");
        assert_eq!(q.name, "权力的游戏");
        assert!(matches!(q.kind, MediaKind::Tv));
        assert_eq!(q.season, None);
    }

    #[test]
    fn clean_title_strips_bare_year_prefix() {
        assert_eq!(clean_title("2006.寂静岭"), ("寂静岭".to_string(), Some(2006)));
        assert_eq!(clean_title("2012.寂静岭2：启示"), ("寂静岭2：启示".to_string(), Some(2012)));
        // 非年份的四位数不误剥（无 . 分隔）
        assert_eq!(clean_title("2020世界"), ("2020世界".to_string(), None));
    }

    #[test]
    fn season_with_subtitle() {
        // 季目录带副标题：第1季：血与沙 → season=1，剧名取上一层
        let q = parse_query("剧集", "剧集/美剧/斯巴达克斯/第1季：血与沙", "S01E01.红蟒.The.Red.Serpent");
        assert_eq!(q.name, "斯巴达克斯");
        assert_eq!(q.season, Some(1));
        assert!(matches!(q.kind, MediaKind::Tv));
    }

    #[test]
    fn movie_subtitle_alt_name() {
        // 副标题片：完整名 + 冒号前主名作 alt_name
        let q = parse_query("电影", "电影/奇幻/指环王", "[2004].指环王3：国王归来");
        assert_eq!(q.name, "指环王3：国王归来");
        assert_eq!(q.alt_name.as_deref(), Some("指环王3"));
        // 无副标题则 alt_name=None
        let q2 = parse_query("电影", "电影/科幻/星球大战", "[1977].星球大战");
        assert_eq!(q2.alt_name, None);
    }

    #[test]
    fn suggest_candidates_splits_punctuation() {
        let c = suggest_candidates("热血热斗！孙悟空与悟饭");
        assert!(c.contains(&"孙悟空与悟饭".to_string()), "应含感叹号后的副名");
        assert!(c.contains(&"热血热斗".to_string()), "应含感叹号前的段");
        assert!(c.contains(&"热血热斗！孙悟空与悟饭".to_string()), "应含完整名");

        let c2 = suggest_candidates("拉欧传.激斗之章");
        assert!(c2.contains(&"拉欧传".to_string()));
        assert!(c2.contains(&"激斗之章".to_string()));
        assert!(c2.contains(&"拉欧传激斗之章".to_string()), "前缀累加拼接");

        // 中点多段
        let c3 = suggest_candidates("热战·烈战·超激战");
        assert!(c3.contains(&"热战".to_string()));
        assert!(c3.contains(&"超激战".to_string()));

        // 去重 + 按长度降序（第一个应是最长的候选）
        let c4 = suggest_candidates("异形前传：普罗米修斯");
        let first_len = c4.first().map(|s| s.chars().count()).unwrap_or(0);
        assert!(c4.iter().all(|s| s.chars().count() <= first_len), "第一个候选应最长");
        // 单字被过滤
        assert!(suggest_candidates("A.蜘蛛侠").iter().all(|s| s.chars().count() >= 2));
    }
}
