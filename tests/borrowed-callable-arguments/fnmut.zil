fn keep<F:FnOnce(i32)->i32>(f:F)->F{return f}
fn step<F:FnMut(i32)->i32>(f:F)->F{var current=f;current(1);return keep(current)}
fn main()->i32{var count=0;var f=move(n:i32)=>{count+=n;return count};var current=step(f);return current(2)-3}
