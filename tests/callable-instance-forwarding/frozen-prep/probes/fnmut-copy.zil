resource R{n:i32}
fn keep<F:FnMut(i32)->i32>(f:F)->F{return f}
fn main()->i32{var count=0;var handler=move(n:i32)=>{count+=n;return count};var current=keep(handler);return current(1)+current(1)-3}
