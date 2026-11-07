fn identity(p:&i32)->&i32{return p}
fn read(p:&i32)->i32{return *p}
fn main()->i32{let x=7;let p=identity(&x);let body=()->i32=>{return 0};let(created,child,parent)=thread_scope(read,p,body);if !created{return 90}return child-7}
