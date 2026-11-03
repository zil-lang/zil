fn read_scalar(value:i32)->i32{return value}
fn read(p:&i32)->i32{return *p}
fn main()->i32{let x=7;let p=&x;let inner=()->i32=>{return *p};let body=()->i32=>{return inner()};let(created,child,parent)=thread_scope(read_scalar,0,body);if !created{return 90}return child+parent-14}
