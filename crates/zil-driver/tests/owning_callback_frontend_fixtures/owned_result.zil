package Reply{manages payload:ByteBuffer;tag:i32}
fn main()->i32{let bytes=bytes_new();if !bytes_reserve(bytes,3){return 1}let handler=move()->Reply=>{return Reply{payload:give bytes,tag:7}};let reply=handler();return reply.tag-7}
