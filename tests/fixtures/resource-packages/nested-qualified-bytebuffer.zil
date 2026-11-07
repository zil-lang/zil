package Child<life a>{manages bytes:ByteBuffer;pointer:&'a mut i32}
package Outer<life a>{child:Child<'a>}
fn make<life a>(pointer:&'a mut i32)->Outer<'a>{
    var bytes=bytes_new()
    bytes_reserve(bytes,1)
    bytes_push(bytes,65u8)
    return Outer::<'a>{child:Child::<'a>{bytes:give bytes,pointer:pointer}}
}
fn bump<life a>(outer:&mut Outer<'a>){
    let pointer=outer.child.pointer
    *pointer=*pointer+1
}
fn main()->i32{
    var value=1
    if true{
        var outer=make(&mut value)
        bump(&mut outer)
    }
    if value==2{return 0}
    return 1
}
