fn bad<life a>(p:&'a i32)->&'a mut i32{return &mut *p}
fn main()->i32{return 0}
