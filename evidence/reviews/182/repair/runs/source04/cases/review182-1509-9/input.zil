extern "C" fn putchar(n:i32)->i32;
resource R{code:i32;n:i32}impl Drop for R{fn drop(self:Self){unsafe{putchar(.code)}}}
fn invoke<F:FnMut()->i32>(f:&mut F)->i32=>f();
fn make()->impl Fn()->i32{let r=R{code:65,n:42};return move ()=>r.n}fn main()->i32{let f=make();let g=()=>f();let n=g();let moved=f;return n+moved()-84}