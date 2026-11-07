package Session<life a>{manages bytes:ByteBuffer;pointer:&'a mut i32}
fn make<life a>(pointer:&'a mut i32)->Session<'a>{return Session::<'a>{bytes:bytes_new(),pointer:pointer}}
fn replace<life a>(old:Session<'a>,next:&'a mut i32)->Session<'a>{
    return Session::<'a>{bytes:bytes_new(),pointer:next}
}
fn main()->i32{
    var first=10
    var second=41
    if true {
        var session=make(&mut first)
        session=replace(session,&mut second)
        let pointer=session.pointer
        *pointer=*pointer+1
    }
    if first==10 && second==42 {return 0}
    return 1
}
