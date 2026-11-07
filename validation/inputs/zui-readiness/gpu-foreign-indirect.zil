fn call(cb:extern "C" fn(c_handle,i32)->i64,context:c_handle)->i64 {unsafe{return cb(context,1)}}
fn main()->i32{return 0}
