fn choose(shared:&i32,unique:&mut i32)->&mut i32{return unique}
fn main()->i32{var x=5;let p=choose(&x,&mut x);return *p}
