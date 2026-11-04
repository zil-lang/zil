struct Tag<life s>{p:&'s i64}
fn helper<life a,life b,F:Fn(&Tag<'b>)->i32>(f:&F,x:&Tag<'a>,y:&Tag<'b>)->i32{return f(y)}
fn main()->i32{return 0}
