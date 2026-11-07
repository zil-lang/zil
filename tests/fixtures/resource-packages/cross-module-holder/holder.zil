pub package Session<life a>{manages bytes:ByteBuffer;pointer:&'a mut i32}
pub fn make<life a>(pointer:&'a mut i32)->Session<'a>{return Session::<'a>{bytes:bytes_new(),pointer:pointer}}
pub fn replace<life a>(session:&mut Session<'a>,next:&'a mut i32){session.pointer=next}
pub fn read<life a>(session:&mut Session<'a>)->i32{let pointer=session.pointer;return *pointer}
