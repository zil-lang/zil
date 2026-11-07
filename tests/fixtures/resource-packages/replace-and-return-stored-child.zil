package Session<life a>{manages bytes:ByteBuffer;pointer:&'a mut i32}
fn make<life a>(pointer:&'a mut i32)->Session<'a>{return Session::<'a>{bytes:bytes_new(),pointer:pointer}}
fn exchange<life a>(session:&mut Session<'a>,next:&'a mut i32)->&'a mut i32{
 session.pointer=next
 return session.pointer
}
fn main()->i32{
 var old=1
 var next=41
 if true{
  var session=make(&mut old)
  let child=exchange(&mut session,&mut next)
  *child=*child+1
  old=7
 }
 if old==7 && next==42{return 0}
 return 1
}
