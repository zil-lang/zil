fn main()->i32 { var n=3; let saved=&n; let f=move(p:&mut i32)->i32=>{*p+=1;return *saved}; return f(&mut n) }
