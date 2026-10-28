fn main()->i32 { var n=3; let saved=&n; let f=move(p:&i32)->i32=>*p+*saved; return f(&n)-6 }
