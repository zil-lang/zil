package Session<life a>{manages bytes:ByteBuffer;pointer:&'a mut i32}
fn make<life a>(pointer:&'a mut i32)->Session<'a>{return Session::<'a>{bytes:bytes_new(),pointer:pointer}}
fn advance<life a>(session:&mut Session<'a>){let p=session.pointer;*p=*p+1;bytes_push(session.bytes,65u8)}
fn relay<life a>(session:Session<'a>)->Session<'a>{return session}
fn main()->i32{var n=41;if true{var session=relay(make(&mut n));advance(&mut session)}return n}
