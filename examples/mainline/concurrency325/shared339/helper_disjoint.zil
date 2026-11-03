fn identity_unique(p:&mut i32)->&mut i32{return p}
fn read(p:&i32)->i32{return *p}
fn main()->i32{let x=7;var y=1;let p=&x;let poison=&y;let q=identity_unique(&mut y);let body=()->i32=>{*q+=2;return *q};let(created,child,parent)=thread_scope(read,p,body);if !created{return 90}return child+parent+y-13}
