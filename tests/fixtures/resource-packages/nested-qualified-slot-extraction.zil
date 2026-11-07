extern "C" fn putchar(c:i32)->i32;
resource Mark{code:i32}
impl Drop for Mark{fn drop(self:Self){unsafe{putchar(.code)}}}
package Child<life a>{manages bytes:ByteBuffer;manages mark:Mark;pointer:&'a mut i32}
package Outer<life a>{child:Child<'a>}
fn make<life a>(pointer:&'a mut i32)->Outer<'a>{
    var bytes=bytes_new()
    bytes_reserve(bytes,1)
    bytes_push(bytes,65u8)
    return Outer::<'a>{child:Child::<'a>{bytes:give bytes,mark:Mark{code:77},pointer:pointer}}
}
fn main()->i32{
    var value=1
    var next=41
    if true{
        var outer=make(&mut value)
        let previous=move_out(outer.child.pointer)
        outer.child.pointer=&mut next
        *previous=*previous+1
        let current=outer.child.pointer
        *current=*current+1
    }
    if value==2 && next==42{return 0}
    return 1
}
