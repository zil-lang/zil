struct Phantom<life s>{n:i32}
fn helper<life s,F:Fn(&Phantom<'s>)->i32>(f:&F,x:&Phantom<'s>)->i32{return f(x)}
fn main()->i32{return 0}
