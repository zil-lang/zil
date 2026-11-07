fn read(p:&i32)->i32{return *p}
fn main()->i32{var x=7;let p=&x;let body=()->i32=>{x=8;return 0};let(created,child,parent)=thread_scope(read,p,body);return 0}
