use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MediaKind {
    Video,
    Comic,
    Game,
}

impl MediaKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            MediaKind::Video => "video",
            MediaKind::Comic => "comic",
            MediaKind::Game => "game",
        }
    }

    pub fn from_kind_str(s: &str) -> crate::error::AppResult<MediaKind> {
        match s {
            "video" => Ok(MediaKind::Video),
            "comic" => Ok(MediaKind::Comic),
            "game" => Ok(MediaKind::Game),
            other => Err(crate::error::AppError::Invalid(format!(
                "unknown media kind: {other}"
            ))),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MediaItem {
    pub id: i64,
    pub kind: MediaKind,
    pub category: String,
    pub category_path: String,
    pub title: String,
    pub subtitle_path: Option<String>,
    pub cover_path: Option<String>,
    pub description: Option<String>,
    pub platform_ok: bool,
    pub exec_path: Option<String>,
    pub playable: bool,
    pub video_path: String, // 拼接出的视频绝对路径，仅 list_media 填充
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn media_kind_serializes_lowercase() {
        let j = serde_json::to_string(&MediaKind::Video).unwrap();
        assert_eq!(j, "\"video\"");
    }

    #[test]
    fn from_kind_str_parses_valid_and_rejects_invalid() {
        assert_eq!(MediaKind::from_kind_str("video").unwrap(), MediaKind::Video);
        assert_eq!(MediaKind::from_kind_str("comic").unwrap(), MediaKind::Comic);
        assert_eq!(MediaKind::from_kind_str("game").unwrap(), MediaKind::Game);
        assert!(MediaKind::from_kind_str("foo").is_err());
    }
}
