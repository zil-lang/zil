fn bad<life a>(p:&'a mut i32)->&'a mut i32{var local=4;return &mut local}
fn main()->i32{return 0}
