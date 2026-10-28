fn apply<F:Fn(i32)->i32>(f:&F,x:i32,flag:bool)->i32{if flag{return f(x)};return f(x+1)}fn main()->i32{let f=move(n:i32)=>n+1;return apply(&f,6,true)-7}
