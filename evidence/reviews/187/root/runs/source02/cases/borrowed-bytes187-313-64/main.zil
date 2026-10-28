
fn call<F:Fn()->i32>(f:&F)->i32=>f();
fn call_mut<F:FnMut()->i32>(f:&mut F)->i32=>f();
fn consume(give b:ByteBuffer){}
fn main()->i32{let a=bytes_new();let b=bytes_new();bytes_reserve(a,1);bytes_push(a,u8(65));bytes_reserve(b,2);let read=a;let edit=a;let f=()=>{let view=bytes_view(read);bytes_push(edit,u8(66));return i32(view[0])};return f()-65}