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
