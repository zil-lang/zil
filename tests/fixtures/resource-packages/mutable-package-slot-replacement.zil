package Session<life a>{manages bytes:ByteBuffer;pointer:&'a mut i32}
fn make<life a>(pointer:&'a mut i32)->Session<'a>{
    return Session::<'a>{bytes:bytes_new(),pointer:pointer}
}
fn replace<life a>(session:&mut Session<'a>,next:&'a mut i32){
    session.pointer=next
}
fn relay<life a>(session:&mut Session<'a>,next:&'a mut i32){
    replace(session,next)
}
fn main()->i32{
    var old=1
    var next=41
    if true {
        var session=make(&mut old)
        relay(&mut session,&mut next)
        old=7
        let pointer=session.pointer
        *pointer=*pointer+1
    }
    if old==7 && next==42 {return 0}
    return 1
}
