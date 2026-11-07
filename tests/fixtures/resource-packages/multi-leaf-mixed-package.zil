package Multi<life a,life b>{manages bytes:ByteBuffer;left:&'a mut i32;right:&'b mut i32}
fn make<life a,life b>(left:&'a mut i32,right:&'b mut i32)->Multi<'a,'b>{
    return Multi::<'a,'b>{bytes:bytes_new(),left:left,right:right}
}
fn apply<life a,life b>(value:&mut Multi<'a,'b>){
    let left=value.left
    let right=value.right
    *left=*left+2
    *right=*right+3
    bytes_push(value.bytes,65u8)
}
fn main()->i32{
    var a=18
    var b=19
    if true {
        var value=make(&mut a,&mut b)
        apply(&mut value)
    }
    if a==20 && b==22 {return 0}
    return 1
}
