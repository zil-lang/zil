fn read_scalar(value:i32)->i32{return value}
fn read(p:&i32)->i32{return *p}
fn run(p:&i32)->i32{let body=()->i32=>{return *p};let(created,child,parent)=thread_scope(read_scalar,0,body);if !created{return 90}return child+parent}
fn main()->i32{let x=8;return run(&x)-16}
