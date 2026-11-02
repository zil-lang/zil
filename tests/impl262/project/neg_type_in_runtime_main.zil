extern "C" fn leak(value:Type)->i32;
fn main()->i32 { unsafe { return leak(#(i32)) } }
