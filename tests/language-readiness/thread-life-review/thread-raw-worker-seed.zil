extern "C" fn external_touch()->i32;
fn unsafe_worker(p:&i32)->i32{unsafe{return external_touch()+*p}}
fn read(p:&i32)->i32{return *p}
fn main()->i32{let x=7;let body=()->i32=>{return 0};let(created,child,parent)=thread_scope(read,&x,body);if !created{return 90}return child-7}
