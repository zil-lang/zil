fn identity(flag:bool)->bool { return flag }
fn main()->i32 { if identity(true) { return 1 }; return 0 }
