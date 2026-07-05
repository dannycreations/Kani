#![cfg(windows)]

/// Converts a Rust string slice into a null-terminated UTF-16 wide string.
pub fn to_wide(s: &str) -> Vec<u16> {
  let mut w = Vec::with_capacity(s.len() + 1);
  w.extend(s.encode_utf16());
  w.push(0);
  w
}
