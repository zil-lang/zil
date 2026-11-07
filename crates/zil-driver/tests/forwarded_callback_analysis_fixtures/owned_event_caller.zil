resource R{n:i32}
package Event{manages bytes:ByteBuffer;n:i32}
package Reply{manages bytes:ByteBuffer;n:i32}
fn keep<F:FnOnce(Event)->Reply>(f:F)->F{return f}
fn main()->i32{
    let r=R{n:7}
    var f=move(e:Event)->Reply=>{return Reply{bytes:give e.bytes,n:e.n+r.n}}
    for i in 0..2{f=keep(f)}
    let p=f(Event{bytes:bytes_new(),n:3})
    return p.n-10
}
