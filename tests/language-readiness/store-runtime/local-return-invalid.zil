fn fake_static()->&[u8] { let local=[65u8]; return &local[..] }
fn main()->i32 { return i32(fake_static()[0]) }
