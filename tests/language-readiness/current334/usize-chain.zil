fn identity(value:usize)->usize{return value}
fn main()->i32{let value:usize=7;if identity(value)!=7{return 1}return 0}
