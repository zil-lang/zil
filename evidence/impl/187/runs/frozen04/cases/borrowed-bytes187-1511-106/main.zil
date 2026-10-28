
fn call<F:Fn()->i32>(f:&F)->i32=>f();
fn call_mut<F:FnMut()->i32>(f:&mut F)->i32=>f();
fn consume(give b:ByteBuffer){}
fn main()->i32{let a=bytes_new();let b=bytes_new();bytes_reserve(a,1);bytes_push(a,u8(65));let read=a;let other=b;let f=()=>{let view=bytes_view_mut(read);bytes_len(other);return i32(view[0])};return call(&f)-65}