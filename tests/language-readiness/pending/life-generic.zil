fn identity<life a>(x:&'a i32)->&'a i32{return x}
fn main()->i32{let n=7;return *identity(&n)-7}
