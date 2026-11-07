extern "C" fn putchar(c:i32)->i32;
resource Mark{code:i32}
impl Drop for Mark{fn drop(self:Self){unsafe{putchar(.code)}}}
enum Fault{Stopped}
package Child<life a>{manages bytes:ByteBuffer;manages mark:Mark;pointer:&'a mut i32}
package Outer<life a>{child:Child<'a>}
fn make<life a>(pointer:&'a mut i32)->Outer<'a>{
    var bytes=bytes_new()
    bytes_reserve(bytes,1)
    bytes_push(bytes,65u8)
    return Outer::<'a>{child:Child::<'a>{bytes:give bytes,mark:Mark{code:70},pointer:pointer}}
}
fn stop_after_make<life a>(pointer:&'a mut i32)->Fault!i32{
    var outer=make(pointer)
    return Fault.Stopped
}
fn main()->i32{
    var value=1
    match stop_after_make(&mut value){
        .ok(_)=>{return 1},
        .err(_)=>{}
    }
    if value==1{return 0}
    return 2
}
