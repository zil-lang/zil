fn identity(value:u64)->u64{return value}
fn main()->i32{let value:u64=7;if identity(value)!=7{return 1}return 0}
