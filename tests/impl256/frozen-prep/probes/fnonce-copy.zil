resource R{n:i32}
fn keep<F:FnOnce(i32)->i32>(f:F)->F{return f}
fn main()->i32{let mark=R{n:1};let handler=move(n:i32)=>{let held=give mark;return n+held.n};let current=keep(handler);return current(1)-2}
