
fn make(n:i32)->impl Fn()->i32{return move()=>n}
fn leaf<F:Fn()->i32>(a:&F,b:&F)->i32{return a()*10+b()}
fn parent<F:Fn()->i32>(a:&F,b:&F)->i32{return leaf(a,b)+leaf(b,a)}
fn main()->i32{let a=make(2);let b=make(7);return parent(&a,&b)-99}
