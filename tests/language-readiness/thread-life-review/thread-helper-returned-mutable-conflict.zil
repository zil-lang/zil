fn identity_unique(p:&mut i32)->&mut i32{return p}
fn read(p:&i32)->i32{return *p}
fn main()->i32{var x=7;let p=&x;let q=identity_unique(&mut x);let body=()->i32=>{*q=8;return 0};let(created,child,parent)=thread_scope(read,p,body);return 0}
