fn edit(buffer: ByteBuffer, s: &[u8]) -> i32 { bytes_reserve(buffer, 8); return i32(s[0]) }
fn main() -> i32 { let buffer = bytes_new(); let other = bytes_new(); bytes_push(other, 7u8); let view = bytes_view(other); return edit(buffer, view) - 7 }
