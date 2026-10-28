
fn make(n:i32)->impl Fn()->i32{return move()=>n}
fn other(n:i32)->impl Fn()->i32{return move()=>n+1}
fn leaf<F:Fn()->i32>(f:&F)->i32=>f()
fn middle<F:Fn()->i32>(f:&F)->i32{let unrelated=other(99);return leaf(&unrelated)}
fn main()->i32{let f=make(4);return middle(&f)}
