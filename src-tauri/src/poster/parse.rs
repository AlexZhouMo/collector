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
}

/// 解析「第N季」「第0N季」中的季号；非季目录返回 None。
fn parse_season(seg: &str) -> Option<u32> {
    let s = seg.strip_prefix('第')?.strip_suffix('季')?;
    let trimmed = s.trim_start_matches('0');
    if trimmed.is_empty() {
        s.parse::<u32>().ok()
    } else {
        trimmed.parse::<u32>().ok()
    }
}

/// 从 title 剥离开头的「[年份].」或「[年份]」前缀，返回 (干净片名, 年份)。
/// 保留片名中间的所有标点（. ： ～ 等）。无「[年份]」前缀则原样返回、year=None。
/// 例：[1993].侏罗纪公园 → ("侏罗纪公园", Some(1993))
///     [2006].致工藤新一的挑战书～离别前的序章 → ("致工藤新一的挑战书～离别前的序章", Some(2006))
///     海贼王（无前缀） → ("海贼王", None)
pub fn clean_title(title: &str) -> (String, Option<u32>) {
    // 匹配开头 "[" + 4 位数字 + "]" + 可选 "."
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
    (title.trim().to_string(), None)
}

/// 从 category / category_path / title 解析出 TMDB 搜索元数据。
/// - 电影：用条目 title（剥离 [年份] 前缀）作片名，Movie，带 year。
/// - 动漫：用条目 title（剥离 [年份] 前缀）作片名，kind=Movie（先搜剧场版）+ is_anime=true
///   （编排层未命中会 fallback 搜 tv），带 year。
/// - 剧集：若末段是「第N季」，剧名取倒数第二段、season=N；否则末段为剧名、season=None。year=None。
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
                MediaQuery { name, kind: MediaKind::Tv, season: Some(season), year: None, is_anime: false }
            } else {
                MediaQuery { name: last.to_string(), kind: MediaKind::Tv, season: None, year: None, is_anime: false }
            }
        }
        "动漫" => {
            let (name, year) = clean_title(title);
            MediaQuery { name, kind: MediaKind::Movie, season: None, year, is_anime: true }
        }
        _ => {
            let (name, year) = clean_title(title);
            MediaQuery { name, kind: MediaKind::Movie, season: None, year, is_anime: false }
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
}
