fn apply<life s>(left:&'s i64)->i32{var count=0;var f=(p:&i64)->i32=>{count+=1;return i32(*p+*left)+count-1};return f(left)}
fn main()->i32{let x:i64=21;return apply(&x)}
