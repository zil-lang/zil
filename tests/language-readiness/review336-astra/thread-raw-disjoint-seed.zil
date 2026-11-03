fn read(p:&i32)->i32{return *p}
fn main()->i32{let x=7;var y=1;let p=&x;let poison=&y;let body=()->i32=>{y+=2;return y};let(created,child,parent)=thread_scope(read,p,body);if !created{return 90}return child+parent+y-13}
