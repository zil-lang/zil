pub fn apply<F:Fn(&i32)->i32>(f:&F,p:&i32)->i32{return f(p)}
