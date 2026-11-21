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
fn exercise<life a>(flag:bool,p1:&'a mut i32,p2:&'a mut i32){
    var b1=bytes_new()
    var b2=bytes_new()
    var x=Outer::<'a>{child:Child::<'a>{bytes:give b1,pointer:p1}}
    var y=Outer::<'a>{child:Child::<'a>{bytes:give b2,pointer:p2}}
    let selected=choose(flag,&mut x,&mut y)
    let pointer=selected.child.pointer
    *pointer=*pointer+1
}
fn main()->i32{
    var left=9
    var right=16
    exercise(true,&mut left,&mut right)
    exercise(false,&mut left,&mut right)
    if left==10 {if right==17{return 0}}
    return 2
}
