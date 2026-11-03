fn relay(p:&mut i32)->&mut i32{return &mut *p}
fn main()->i32{var x=5;let p=&mut x;let q=relay(&mut *p);*p=9;return *q}
