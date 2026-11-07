resource Mark{code:i32}
fn keep<F:FnOnce()->i32>(f:F)->F{return f}
fn main()->i32{let mark=Mark{code:65};let handler=move()=>mark.code;let current=keep(handler);return handler()+current()-130}
