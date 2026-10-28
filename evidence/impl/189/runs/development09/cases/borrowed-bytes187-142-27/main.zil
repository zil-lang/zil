
fn call<F:Fn()->i32>(f:&F)->i32=>f();
fn call_mut<F:FnMut()->i32>(f:&mut F)->i32=>f();
fn consume(give b:ByteBuffer){}
fn main()->i32{let b=bytes_new();if !bytes_reserve(b,1){return 7};bytes_push(b,u8(65));let d=b;let view=bytes_view(b);let f=()=>i32(bytes_len(d)+bytes_capacity(d))+i32(bytes_view(d)[0]);return call(&f)+i32(view[0])-132}