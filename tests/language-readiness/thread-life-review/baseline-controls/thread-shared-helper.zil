fn read_scalar(value:i32)->i32{return value}
fn helper(p:&i32)->i32{let q=&*p;return *q+1}
fn read(p:&i32)->i32{let q=p;return helper(q)}
fn main()->i32{let x=6;let p=&x;let body=()->i32=>{return *p};let(created,child,parent)=thread_scope(read_scalar,0,body);if !created{return 90}return child+parent-13}
