fn make(n:i32)->impl Fn()->&i32{return move()=>&n}
fn read(p:&i32)->i32{return *p}
fn main()->i32{let owner=make(7);let p=owner();let body=()->i32=>{return 0};let(created,child,parent)=thread_scope(read,p,body);if !created{return 90}return child-7}
