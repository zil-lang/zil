fn answer(n:i32)->i32=>n+1
resource R{call:fn(i32)->i32;n:i32}
fn read(r:R)->i32=>r.call(r.n)
fn make()->impl Fn()->i32{let r=R{call:answer,n:41};return move()=>read(r)}
fn main()->i32{let f=make();return f()+f()-84}
