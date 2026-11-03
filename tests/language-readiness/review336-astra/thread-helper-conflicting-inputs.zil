fn read(p:&i32)->i32{return *p}
fn run(p:&i32,q:&mut i32)->i32{let body=()->i32=>{*q=8;return 0};let(created,child,parent)=thread_scope(read,p,body);return 0}
fn main()->i32{var x=7;return run(&x,&mut x)}
