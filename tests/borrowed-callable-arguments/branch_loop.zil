fn keep<F:FnOnce(i32)->i32>(flag:bool,f:F)->F{return f}
fn cycle<F:FnOnce(i32)->i32>(f:F,count:i32)->F{var current=f;var i=0;while i<count{current=keep(true,current);i+=1}return current}
fn main()->i32{let f=(n:i32)=>n+3;let current=cycle(keep(true,f),2);return current(4)-7}
