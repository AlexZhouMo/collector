//! 字幕文件编码检测与解码：UTF-8（含 BOM）/ UTF-16（BOM）/ GBK 兜底。
use encoding_rs::{GBK, UTF_16BE, UTF_16LE, UTF_8};

/// 按 BOM 与合法性检测编码并解码为 String。全部失败返回 None。
pub fn decode_bytes(bytes: &[u8]) -> Option<String> {
    // BOM 检测
    if bytes.starts_with(&[0xFF, 0xFE]) {
        let (s, _, had_err) = UTF_16LE.decode(&bytes[2..]);
        if !had_err {
            return Some(s.into_owned());
        }
    }
    if bytes.starts_with(&[0xFE, 0xFF]) {
        let (s, _, had_err) = UTF_16BE.decode(&bytes[2..]);
        if !had_err {
            return Some(s.into_owned());
        }
    }
    // UTF-8（encoding_rs 会跳过 UTF-8 BOM）
    let (s, _, had_err) = UTF_8.decode(bytes);
    if !had_err {
        return Some(s.into_owned());
    }
    // GBK 兜底
    let (s, _, had_err) = GBK.decode(bytes);
    if !had_err {
        return Some(s.into_owned());
    }
    None
}

/// 读文件并解码。读失败或解码失败返回 None。
pub fn read_subtitle(path: &std::path::Path) -> Option<String> {
    let bytes = std::fs::read(path).ok()?;
    decode_bytes(&bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn decodes_utf8() {
        assert_eq!(decode_bytes(b"\xe4\xbd\xa0\xe5\xa5\xbd"), Some("你好".to_string())); // "你好" UTF-8
    }
    #[test]
    fn decodes_utf8_bom() {
        assert_eq!(decode_bytes(b"\xef\xbb\xbf\xe4\xbd\xa0"), Some("你".to_string()));
    }
    #[test]
    fn decodes_utf16le_bom() {
        // "你" UTF-16LE with BOM: FF FE 60 4F
        assert_eq!(decode_bytes(&[0xFF, 0xFE, 0x60, 0x4F]), Some("你".to_string()));
    }
    #[test]
    fn decodes_gbk_fallback() {
        // "你好" GBK: C4 E3 BA C3
        assert_eq!(decode_bytes(&[0xC4, 0xE3, 0xBA, 0xC3]), Some("你好".to_string()));
    }
}
