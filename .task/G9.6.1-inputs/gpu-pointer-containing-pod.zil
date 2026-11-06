struct Upload {data:&u8;length:i32}
extern "C" fn upload(value:&Upload)->i32 contract{provides(noescape(value));effects(ffi);};
fn main()->i32{return 0}
