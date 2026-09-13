//! 字幕分类：Title(含《》)/Note(被（）括起)/Default(其余)。

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum Style { Default, Title, Note }

impl Style {
    pub fn name(&self) -> &'static str {
        match self {
            Style::Default => "Default",
            Style::Title => "Title",
            Style::Note => "Note",
        }
    }
}

/// 按中文段的字符标志分类。has_en 暂不影响分类（双/单行由组装阶段按有无英文决定）。
pub fn classify_style(zh: &str, _has_en: bool) -> Style {
    let z = zh.trim();
    if z.contains('《') || z.contains('》') {
        Style::Title
    } else if z.starts_with('（') && z.ends_with('）') {
        Style::Note
    } else {
        Style::Default
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn classify_by_markers() {
        assert_eq!(classify_style("《肖申克的救赎》", false), Style::Title);
        assert_eq!(classify_style("（一把钥匙）", false), Style::Note);
        assert_eq!(classify_style("普通台词", true), Style::Default);
    }
}
