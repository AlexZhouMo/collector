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
