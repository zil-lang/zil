fn set(n:&mut i32)->i32{*n=9;return 0}
fn leaf<F:FnMut()->i32>(f:&mut F,n:i32)->i32=>f()+n
fn run<F:FnMut()->i32>(f:&mut F,n:i32)->i32=>leaf(f,n)
fn main()->i32{var n=0;var f=()=>{n+=1;return n};return run(&mut f,set(&mut n))}