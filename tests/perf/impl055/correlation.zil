resource T{n:i32}
enum E{Ok(i32),Err(i32)}
fn f(flag:bool)->E{let a=T{n:1};let b=T{n:2};let fa=()=>a.n;let fb=()=>b.n;if flag{return E.Ok(fa())}return E.Err(fb())}
fn main()->i32{return 0}
