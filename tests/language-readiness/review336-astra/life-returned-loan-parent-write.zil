fn relay<life a>(p:&'a mut i32)->&'a mut i32{return &mut *p}
fn main()->i32{var x=5;let p=&mut x;let q=relay(&mut *p);*p=9;return *q}
