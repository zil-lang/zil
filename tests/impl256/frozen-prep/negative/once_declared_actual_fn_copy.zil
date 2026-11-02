fn keep<F:FnOnce(i32)->i32>(f:F)->F{return f}
fn outer<F:FnOnce(i32)->i32>(f:F)->i32{
    let current=keep(f)
    let first=current
    let second=current
    return first(1)+second(2)
}
fn main()->i32{let handler=(value:i32)=>value;return outer(handler)}
