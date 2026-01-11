/// 按 UTF-8 字符边界安全截断字符串，避免跨多字节字符导致的切片 panic
pub fn truncate_utf8(s: &str, max: usize) -> &str {
  if s.len() <= max {
    return s;
  }
  let mut end = max.min(s.len());
  while end > 0 && !s.is_char_boundary(end) {
    end -= 1;
  }
  &s[..end]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_truncate_utf8_ascii() {
        assert_eq!(truncate_utf8("hello", 3), "hel");
        assert_eq!(truncate_utf8("hello", 10), "hello");
        assert_eq!(truncate_utf8("hello", 5), "hello");
    }

    #[test]
    fn test_truncate_utf8_multibyte() {
        // 中文字符,每个占3字节
        let s = "你好世界";
        assert_eq!(truncate_utf8(s, 6), "你好");
        assert_eq!(truncate_utf8(s, 7), "你好"); // 7不是字符边界,回退到6
        assert_eq!(truncate_utf8(s, 3), "你");
        assert_eq!(truncate_utf8(s, 100), "你好世界");
    }

    #[test]
    fn test_truncate_utf8_mixed() {
        let s = "Hello世界";
        assert_eq!(truncate_utf8(s, 5), "Hello");
        assert_eq!(truncate_utf8(s, 8), "Hello世");
        assert_eq!(truncate_utf8(s, 100), "Hello世界");
    }

    #[test]
    fn test_truncate_utf8_emoji() {
        let s = "😀😁😂";
        // Emoji占4字节
        assert_eq!(truncate_utf8(s, 4), "😀");
        assert_eq!(truncate_utf8(s, 8), "😀😁");
    }

    #[test]
    fn test_truncate_utf8_empty() {
        assert_eq!(truncate_utf8("", 10), "");
        assert_eq!(truncate_utf8("test", 0), "");
    }
}
