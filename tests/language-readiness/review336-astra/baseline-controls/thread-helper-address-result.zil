fn read_scalar(value:i32)->i32{return value}
fn identity(p:&i32)->&i32{return p}
fn read(p:&i32)->i32{return *identity(p)}
fn main()->i32{let x=7;let body=()->i32=>{return 0};let(created,child,parent)=thread_scope(read_scalar,0,body);return 0}
