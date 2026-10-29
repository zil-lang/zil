extern "C" fn putchar(n:i32)->i32;fn emit(n:i32){unsafe{putchar(n)}}
resource Mark{code:i32}impl Drop for Mark{fn drop(self:Self){emit(.code)}}
fn choose<F:FnOnce()->i32>(f:F,n:i32)->i32{for index in 0..n{return f()}return 0}
fn main()->i32{let mark=Mark{code:77};let f=move()=>mark.code;return choose(f,0)-0}
