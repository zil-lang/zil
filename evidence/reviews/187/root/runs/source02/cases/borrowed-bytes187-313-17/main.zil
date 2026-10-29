
fn call<F:Fn()->i32>(f:&F)->i32=>f();
fn call_mut<F:FnMut()->i32>(f:&mut F)->i32=>f();
fn consume(give b:ByteBuffer){}
fn main()->i32{let b=bytes_new();bytes_reserve(b,1);bytes_push(b,u8(65));let d=b;let view=bytes_view(b);let f=()=>{bytes_view_mut(d);return 0};let answer=f();return answer+i32(view[0])-65}