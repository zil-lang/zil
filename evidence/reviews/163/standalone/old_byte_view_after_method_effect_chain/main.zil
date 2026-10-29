impl ByteBuffer(self:Self){fn grow()->bool=>bytes_reserve(self,64);fn through()->bool=>self.grow()}
fn wrapper(b:ByteBuffer){b.through()}
fn main()->i32{let b=bytes_new();bytes_reserve(b,8);bytes_push(b,42u8);let view=bytes_view(b);wrapper(b);return i32(view[0])}
