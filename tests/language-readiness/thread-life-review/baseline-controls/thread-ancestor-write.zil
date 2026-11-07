fn read_scalar(value:i32)->i32{return value}
fn read(p:&i32)->i32{return *p}
fn main()->i32{var x=7;let p=&mut x;let q=&*p;let body=()->i32=>{*p=8;return 0};let(created,child,parent)=thread_scope(read_scalar,0,body);return 0}
