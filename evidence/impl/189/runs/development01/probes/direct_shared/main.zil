fn main()->i32 { let n=3; let f=(p:&i32)->i32=>*p; return f(&n)-3 }
