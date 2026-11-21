fn make(value:u8)->impl Fn()->&[u8]{let buffer=bytes_new();bytes_reserve(buffer,1);bytes_push(buffer,value);return move()=>bytes_view(buffer)}
fn keep<F:Fn()->&[u8]>(input:F)->F{return input}
fn relay<F:Fn()->&[u8]>(input:F)->F{return keep(input)}
fn main()->i32{let first=relay(make(20u8));let second=relay(make(22u8));let left=first();let right=second();return i32(left[0])+i32(right[0])-42}
