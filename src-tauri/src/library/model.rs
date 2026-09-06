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
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MediaItem {
    pub id: i64,
    pub kind: MediaKind,
    pub category: String,
    pub category_path: String,
    pub title: String,
    pub path: String,
    pub subtitle_path: Option<String>,
    pub cover_path: Option<String>,
    pub description: Option<String>,
    pub platform_ok: bool,
    pub exec_path: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn media_kind_serializes_lowercase() {
        let j = serde_json::to_string(&MediaKind::Video).unwrap();
        assert_eq!(j, "\"video\"");
    }
}
