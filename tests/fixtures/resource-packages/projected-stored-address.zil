package Child<life a>{manages bytes:ByteBuffer;pointer:&'a mut i32}
package Outer<life a>{left:Child<'a>;right:Child<'a>}
fn make<life a>(left:&'a mut i32,right:&'a mut i32)->Outer<'a>{
    var a=bytes_new()
    var b=bytes_new()
    return Outer::<'a>{left:Child::<'a>{bytes:give a,pointer:left},right:Child::<'a>{bytes:give b,pointer:right}}
}
fn bump<life a>(outer:&mut Outer<'a>){
    let pointer=outer.left.pointer
    *pointer=*pointer+1
}
fn main()->i32{
    var left=1
    var right=5
    if true {
        var outer=make(&mut left,&mut right)
        bump(&mut outer)
    }
    if left==2 {if right==5{return 0}}
    return 1
}
