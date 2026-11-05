struct Tag<life s>{p:&'s i64}
fn helper<life s,F:Fn(&Tag<'s>)->i32>(f:&F,x:&Tag<'s>)->i32{return #(42)}
fn main()->i32{return 0}
