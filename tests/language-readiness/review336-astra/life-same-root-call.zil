fn choose<life a>(shared:&'a i32,unique:&'a mut i32)->&'a mut i32{return unique}
fn main()->i32{var x=5;let p=choose(&x,&mut x);return *p}
