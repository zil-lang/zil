
fn leaf<F:Fn(i32)->i32>(f:&F,x:i32)->i32=>f(x)
fn inner<F:Fn(i32)->i32>(f:&F,x:i32)->i32=>leaf(f,x)
fn outer<F:Fn(i32)->i32>(f:&F,x:i32)->i32=>inner(f,x)
fn main()->i32{let f=move(n:i32)=>n+1;return outer(&f,6)-7}
