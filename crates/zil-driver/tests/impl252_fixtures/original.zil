fn keep<F:FnOnce(i32)->i32>(f:F)->F{return f}
fn main()->i32{let handler=keep(move(value:i32)->i32=>{return value+3});return handler(4)-7}
