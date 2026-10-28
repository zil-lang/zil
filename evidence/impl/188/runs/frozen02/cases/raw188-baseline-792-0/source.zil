
fn make(n:i32)->impl Fn()->i32{return move()=>n}
fn leaf<F:Fn()->i32>(f:&F)->i32=>f()
fn middle<F:Fn()->i32>(f:&F)->i32{let unrelated=make(99);let ignored=&unrelated;ignored;return leaf(f)}
fn main()->i32{let f=make(4);return middle(&f)-4}
