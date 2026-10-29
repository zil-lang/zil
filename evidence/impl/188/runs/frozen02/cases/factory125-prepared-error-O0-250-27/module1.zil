pub fn apply<F:Fn(i64)->i64>(f:&F,n:i64)->i64{return f(n)}
 pub fn twice<F:FnMut(i64)->i64>(f:&mut F)->i64{return f(1)+f(2)}
 pub fn once<F:FnOnce(i64)->i64>(f:F,n:i64)->i64{return f(n)}
 pub fn once_error<F:FnOnce()->bool!i64>(f:F)->bool!i64{return f()}