fn keep<F:FnOnce()->i32>(f:F)->F{return f}
fn main()->i32{let value=7;let view=&value;let handler=move()=>*view;let current=keep(handler);return current()-7}
