import std.string
fn main()->string.Error!i32 {let bytes=[65u8];let label=string.from_utf8(&bytes[..],64)?;return i32(string.byte_len(&label))}
