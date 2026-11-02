fn wrong<F:FnOnce()->i32>(f:F)->F{f();return f}
fn main()->i32{let current=wrong(()=>1);return current()}
