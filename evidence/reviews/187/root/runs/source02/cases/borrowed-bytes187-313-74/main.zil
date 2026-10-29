
fn call<F:Fn()->i32>(f:&F)->i32=>f();
fn call_mut<F:FnMut()->i32>(f:&mut F)->i32=>f();
fn consume(give b:ByteBuffer){}
fn read(b:ByteBuffer)->i32=>i32(bytes_len(b));fn forward(b:ByteBuffer)->i32=>read(b);fn clear(b:ByteBuffer)->bool=>bytes_truncate(b,0);fn clear_again(b:ByteBuffer)->bool=>clear(b);fn main()->i32{let b=bytes_new();bytes_reserve(b,1);bytes_push(b,u8(65));let d=b;let view=bytes_view(b);let f=()=>forward(d);return call(&f)+i32(view[0])-66}