#[derive(Debug, Clone)]
pub struct ScannedItem {
    pub category: String,
    pub category_path: String,
    pub title: String,
    pub cover_path: Option<String>,
    pub description: Option<String>,
}
