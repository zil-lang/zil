
fn call<F:Fn()->i32>(f:&F)->i32=>f();
fn call_mut<F:FnMut()->i32>(f:&mut F)->i32=>f();
fn consume(give b:ByteBuffer){}
fn main()->i32{let b=bytes_new();bytes_reserve(b,1);bytes_push(b,u8(65));let d=b;let view=bytes_view(b);var n=0;var f=()=>{n+=1;return i32(bytes_len(d))+n};let one=call_mut(&mut f);let two=f();let byte=i32(view[0]);if !bytes_reserve(b,4){return 7};bytes_truncate(b,0);n+=1;return one+two+byte+n-73}