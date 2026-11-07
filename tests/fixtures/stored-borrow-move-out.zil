package Session<life a>{manages bytes:ByteBuffer;pointer:&'a mut i32}
fn make<life a>(pointer:&'a mut i32)->Session<'a>{
 return Session::<'a>{bytes:bytes_new(),pointer:pointer}
}
fn exchange<life a>(session:&mut Session<'a>,next:&'a mut i32)->&'a mut i32{
 let previous=move_out(session.pointer)
 session.pointer=next
 return previous
}
fn main()->i32{
 var old=1
 var next=41
 if true{
  var session=make(&mut old)
  let previous=exchange(&mut session,&mut next)
  *previous=*previous+1
  let current=session.pointer
  *current=*current+1
 }
 if old==2 && next==42{return 0}
 return 1
}
