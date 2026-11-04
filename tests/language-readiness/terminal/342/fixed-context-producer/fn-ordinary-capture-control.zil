fn apply<life s>(left:&'s i64)->i32{let f=(p:&i64)->i32=>i32(*p+*left);return f(left)}
fn main()->i32{let x:i64=21;return apply(&x)}
