fn keep<F:FnOnce(i32)->i32>(count:i32,f:F)->F{var current=f;for i in 0..count{current=inner(current)}return current}
fn inner<F:FnOnce(i32)->i32>(f:F)->F{return f}
fn outer<F:FnOnce(i32)->i32>(f:F)->i32{let current=keep(3,f);return current(1)+current(2)}
fn main()->i32{return outer((n:i32)=>n)}
