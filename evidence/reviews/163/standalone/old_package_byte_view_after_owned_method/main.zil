package P{manages b:ByteBuffer}impl P{fn finish(self:Self)->i64=>bytes_len(.b)}
fn main()->i32{let b=bytes_new();bytes_reserve(b,8);bytes_push(b,42u8);let p=P{b:give b};let view=bytes_view(p.b);p.finish();return i32(view[0])}
