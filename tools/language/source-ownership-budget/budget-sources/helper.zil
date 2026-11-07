fn twice<F:Fn(i32)->i32>(f:&F,n:i32)->i32{return f(n)+f(n)}
fn make(n:i32)->impl Fn(i32)->i32{return move (x)=>n+x}
fn main()->i32{let f=make(20);return twice(&f,1)-42}
