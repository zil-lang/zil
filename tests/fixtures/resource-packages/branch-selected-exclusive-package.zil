package Child<life a>{manages bytes:ByteBuffer;pointer:&'a mut i32}
package Outer<life a>{child:Child<'a>}
fn make<life a>(pointer:&'a mut i32)->Outer<'a>{
    var bytes=bytes_new()
    bytes_reserve(bytes,1)
    bytes_push(bytes,65u8)
    return Outer::<'a>{child:Child::<'a>{bytes:give bytes,pointer:pointer}}
}
fn choose<life a,life b>(flag:bool,left:&'b mut Outer<'a>,right:&'b mut Outer<'a>)->&'b mut Outer<'a>{
    if flag{return left}
    return right
}
fn bump<life a,life b>(left:&'b mut Outer<'a>,right:&'b mut Outer<'a>){
    let selected=choose(true,left,right)
    let pointer=selected.child.pointer
    *pointer=*pointer+1
    let other=left.child.pointer
    *other=17
}
fn main()->i32{return 0}
