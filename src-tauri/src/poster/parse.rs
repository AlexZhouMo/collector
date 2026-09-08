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

/// 从 category / category_path / title 解析出 TMDB 搜索元数据。
/// - 电影：取 category_path 末段作片名，Movie。
/// - 动漫：取 category_path 末段作名，Tv（动漫按剧搜命中率更高）。
/// - 剧集：若末段是「第N季」，剧名取倒数第二段、season=N；否则末段为剧名、season=None。
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
                MediaQuery { name, kind: MediaKind::Tv, season: Some(season) }
            } else {
                MediaQuery { name: last.to_string(), kind: MediaKind::Tv, season: None }
            }
        }
        "动漫" => MediaQuery { name: last.to_string(), kind: MediaKind::Tv, season: None },
        _ => MediaQuery { name: last.to_string(), kind: MediaKind::Movie, season: None },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn movie_takes_last_segment() {
        let q = parse_query("电影", "电影/科幻/星球大战", "星球大战.国语");
        assert_eq!(q.name, "星球大战");
        assert!(matches!(q.kind, MediaKind::Movie));
        assert_eq!(q.season, None);
    }

    #[test]
    fn tv_normal_season() {
        let q = parse_query("剧集", "剧集/英剧/神探夏洛克/第2季", "S02E01");
        assert_eq!(q.name, "神探夏洛克");
        assert!(matches!(q.kind, MediaKind::Tv));
        assert_eq!(q.season, Some(2));
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
    fn anime_is_tv() {
        let q = parse_query("动漫", "动漫/热血/海贼王", "x");
        assert_eq!(q.name, "海贼王");
        assert!(matches!(q.kind, MediaKind::Tv));
        assert_eq!(q.season, None);
    }
}
