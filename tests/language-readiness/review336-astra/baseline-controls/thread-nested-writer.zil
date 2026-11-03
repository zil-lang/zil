fn read_scalar(value:i32)->i32{return value}
fn read(p:&i32)->i32{return *p}
fn main()->i32{var x=7;let p=&x;var inner=()->i32=>{x+=1;return x};let body=()->i32=>{return inner()};let(created,child,parent)=thread_scope(read_scalar,0,body);return 0}
