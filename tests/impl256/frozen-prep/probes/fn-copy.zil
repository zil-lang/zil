resource R{n:i32}
fn keep<F:Fn(i32)->i32>(f:F)->F{return f}
fn main()->i32{let handler=(n:i32)=>n+1;let current=keep(handler);return current(1)+current(2)-5}
