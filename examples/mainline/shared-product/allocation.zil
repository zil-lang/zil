extern "C" fn putchar(c:i32)->i32;
fn emit(c:i32){unsafe{putchar(c)}}
resource R{n:i32}
impl Drop for R{fn drop(self:Self){emit(.n)}}
package Leaf{manages buffer:ByteBuffer;manages marker:R;value:i32}
package Root{child:Leaf;tag:i32}
fn value(p:&Root)->i32{return p.child.value+p.child.marker.n+p.tag}
fn main()->i32{
    let bytes=bytes_new()
    if !bytes_reserve(bytes,16){emit(70);return 10}
    let root=Root{child:Leaf{buffer:give bytes,marker:R{n:65},value:7},tag:11}
    let p=&root
    let n=value(p)
    let extra=bytes_new()
    if !bytes_reserve(extra,32){emit(71);return 11}
    emit(88)
    return n-83
}
