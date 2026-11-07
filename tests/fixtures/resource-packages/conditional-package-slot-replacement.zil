package Session<life a>{manages bytes:ByteBuffer;pointer:&'a mut i32}
fn make<life a>(pointer:&'a mut i32)->Session<'a>{
    return Session::<'a>{bytes:bytes_new(),pointer:pointer}
}
fn maybe_replace<life a>(session:&mut Session<'a>,next:&'a mut i32,condition:bool){
    if condition {session.pointer=next}
}
fn main()->i32{
    var old=1
    var next=41
    var session=make(&mut old)
    maybe_replace(&mut session,&mut next,true)
    old=7
    let pointer=session.pointer
    *pointer=*pointer+1
    return 0
}
