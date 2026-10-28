
fn leaf<F:Fn(i32)->i32>(f:&F,n:i32,flag:bool)->i32{if flag{return f(n)};return 0}
fn inner<F:Fn(i32)->i32>(f:&F,n:i32,flag:bool)->i32{let a=leaf(f,n,flag);if flag{return a};return f(n+1)+f(n+2)}
fn outer<F:Fn(i32)->i32>(f:&F,n:i32,flag:bool)->i32=>inner(f,n,flag)+leaf(f,n,!flag)
fn main()->i32{let f=move(n:i32)=>n+1;return outer(&f,3,false)-15}
