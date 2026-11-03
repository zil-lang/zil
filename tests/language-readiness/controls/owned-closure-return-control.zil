resource Model{n:i32}
fn make(seed:i32)->impl FnMut(i32)->i32{let model=Model{n:seed};var count=0;return move(value:i32)->i32=>{count+=1;return value+model.n+count}}
fn main()->i32{var handler=make(10);let first=handler(1);let second=handler(2);return first+second-26}
