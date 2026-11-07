package Session<life a>{manages bytes:ByteBuffer;pointer:&'a mut i32}
fn make<life a>(pointer:&'a mut i32)->Session<'a>{
 var bytes=bytes_new()
 bytes_reserve(bytes,1)
 bytes_push(bytes,65u8)
 return Session::<'a>{bytes:give bytes,pointer:pointer}
}
fn main()->i32{
 var old=1
 if true{
  var session=make(&mut old)
  let previous=move_out(session.pointer)
  *previous=2
 }
 if old==2{return 0}
 return 1
}
