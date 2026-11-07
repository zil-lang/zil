fn step<F:FnMut(i32)->i32>(f:F)->F{
    var current=f
    current(5)
    return current
}
fn main()->i32{
    var count=1
    var handler=move(value:i32)->i32=>{count+=value;return count}
    var current=step(handler)
    return current(2)-8
}
