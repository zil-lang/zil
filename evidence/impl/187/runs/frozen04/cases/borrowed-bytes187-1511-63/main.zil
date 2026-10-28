
fn call<F:Fn()->i32>(f:&F)->i32=>f();
fn call_mut<F:FnMut()->i32>(f:&mut F)->i32=>f();
fn consume(give b:ByteBuffer){}
fn main()->i32{let b=bytes_new();bytes_reserve(b,1);bytes_push(b,u8(65));let f=()=>i32(bytes_len(b));let view=bytes_view_mut(b);consume(give b);return i32(view[0])-65}