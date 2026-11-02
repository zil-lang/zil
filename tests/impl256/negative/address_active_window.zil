resource R{n:i32}
fn keep<F:FnMut(&mut i32)->i32>(f:F)->F{return f}
fn main()->i32{
    let r=R{n:1}
    var count=0
    var handler=move(value:&mut i32)->i32=>{let n=r.n;count+=1;*value+=count;return *value}
    var value=40
    let view=&value
    var current=keep(handler)
    let changed=current(&mut value)
    return *view+changed
}
