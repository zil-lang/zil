import std.string
fn main()->string.Error!i32 {let label=string.from_utf8("按钮",64)?;return i32(string.byte_len(&label))}
