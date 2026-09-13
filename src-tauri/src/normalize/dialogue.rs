//! 对话结构规整：只规整已标注的 `-`（统一 "- " 空格），不臆测哪句是对话。

/// 把已有的对话破折号统一为 "- "（破折号后恰一个空格）。
/// 只处理行首或空格后的 '-'（对话标记），不动连字符/减号中的 '-'。
pub fn regularize_dash(s: &str) -> String {
    let chars: Vec<char> = s.chars().collect();
    let mut res = String::with_capacity(s.len());
    let mut i = 0;
    while i < chars.len() {
        let at_dialogue_pos = chars[i] == '-' && (i == 0 || chars[i - 1] == ' ');
        if at_dialogue_pos {
            res.push('-');
            res.push(' ');
            i += 1;
            // 跳过 '-' 后原有空格，避免 "- " 变 "-  "
            while i < chars.len() && chars[i] == ' ' { i += 1; }
        } else {
            res.push(chars[i]);
            i += 1;
        }
    }
    res.trim().to_string()
}

/// 规整已标注的标记：中文括号 ()→（）并去内侧空格；[]、# 保留（仅去紧贴内侧空格）。
/// 不臆测哪行该加标记——那由质检提示。仅应作用于中文段（英文段括号保留半角，由调用方保证）。
pub fn regularize_markers(s: &str) -> String {
    let mut t = s.to_string();
    // 中文括号全角化 + 去内侧空格
    t = t.replace("( ", "（").replace(" )", "）").replace('(', "（").replace(')', "）");
    // 方括号（外语）、井号（歌曲）保留，仅去紧贴内侧空格
    t = t.replace("[ ", "[").replace(" ]", "]");
    t.trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn regularize_dash_spacing() {
        assert_eq!(regularize_dash("-你好 -再见"), "- 你好 - 再见");
        assert_eq!(regularize_dash("-Hi -Bye"), "- Hi - Bye");
    }
    #[test]
    fn no_dash_unchanged() {
        assert_eq!(regularize_dash("普通一句"), "普通一句");
    }
}

#[cfg(test)]
mod marker_tests {
    use super::*;
    #[test]
    fn regularize_paren_spacing() {
        assert_eq!(regularize_markers("( 道具 )"), "（道具）");
    }
    #[test]
    fn keep_song_and_foreign() {
        assert_eq!(regularize_markers("#歌词"), "#歌词");
        assert_eq!(regularize_markers("[Hola]"), "[Hola]");
    }
}
