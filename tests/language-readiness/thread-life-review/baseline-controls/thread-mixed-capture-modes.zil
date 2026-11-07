fn read_scalar(value:i32)->i32{return value}
fn read(p:&i32)->i32{return *p}
fn main()->i32{let x=7;var y=1;let p=&x;let body=()->i32=>{y+=2;return *p+y};let(created,child,parent)=thread_scope(read_scalar,0,body);if !created{return 90}return child+parent+y-20}
