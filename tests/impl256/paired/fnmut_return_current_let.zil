fn keep<F:FnOnce(i32)->i32>(f:F)->F{return f}
fn outer<F:FnMut(i32)->i32>(f:F)->F{
    let current=keep(f)
    current(1)
    return current
}
fn main()->i32{let handler=(value:i32)=>value;var current=outer(handler);return current(1)-1}
