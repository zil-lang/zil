fn main() -> i32 { let buffer = bytes_new(); bytes_push(buffer, 7u8); let view = bytes_view(buffer); let f = (s: &[u8]) => i32(bytes_len(buffer)) + i32(s[0]); return f(view) - 8 }
