fn wrong<F:FnOnce()->i32>(flag:bool,f:F)->F{if flag{f()}return f}
fn main()->i32{let current=wrong(true,()=>1);return current()}
