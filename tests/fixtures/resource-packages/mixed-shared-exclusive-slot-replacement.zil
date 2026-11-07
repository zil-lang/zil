package Session<life a,life b>{manages bytes:ByteBuffer;left:&'a i32;right:&'b mut i32}
fn make<life a,life b>(left:&'a i32,right:&'b mut i32)->Session<'a,'b>{
    return Session::<'a,'b>{bytes:bytes_new(),left:left,right:right}
}
fn replace<life a,life b>(session:&mut Session<'a,'b>,next:&'b mut i32){
    session.right=next
}
fn main()->i32{
    var left=1
    var old=2
    var next=3
    if true {
        var session=make(&left,&mut old)
        let detached=session.left
        replace(&mut session,&mut next)
        old=7
        if *detached!=1{return 1}
        let current=session.right
        *current=*current+1
    }
    if old==7 && next==4{return 0}
    return 2
}
