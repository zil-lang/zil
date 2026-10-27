import extension;impl ByteBuffer(self:Self){fn size()->i64=>bytes_len(self);fn append(v:u8)->bool=>bytes_push(self,v)}
resource R{n:i32}impl R(self:Self){fn size()->i64=>100}
fn main()->i32{let b=bytes_new();bytes_reserve(b,4);b.append(42u8);return i32(b.size())-1}
