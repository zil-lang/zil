resource Mark{n:i32}
fn apply<life s>(left:&'s i64)->i32{let mark=Mark{n:1};let f=move(p:&i64)->i32=>{let taken=give mark;return i32(*p+*left)+taken.n-1};return f(left)}
fn main()->i32{let x:i64=21;return apply(&x)}
