fn read(p:&i32)->i32{return *p}
fn main()->i32{let x=9;let parent=move()->i32=>{let body=()->i32=>{return x};let(created,child,own)=thread_scope(read,&x,body);if !created{return 90}return child+own};return parent()-18}
