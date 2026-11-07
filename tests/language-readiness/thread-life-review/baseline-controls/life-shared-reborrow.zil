fn freeze(p:&mut i32)->&i32{return &*p}
fn main()->i32{var x=12;let p=freeze(&mut x);return *p-12}
