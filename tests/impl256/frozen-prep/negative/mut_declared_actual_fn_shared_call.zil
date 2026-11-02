fn keep<F:FnOnce(i32)->i32>(f:F)->F{return f}
fn outer<F:FnMut(i32)->i32>(f:F)->i32{
    let current=keep(f)
    return current(1)
}
fn main()->i32{let handler=(value:i32)=>value;return outer(handler)-1}
