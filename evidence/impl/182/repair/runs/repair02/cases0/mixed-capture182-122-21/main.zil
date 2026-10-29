
extern "C" fn putchar(x:i32)->i32;
resource R{code:i32;n:i32}
impl Drop for R{fn drop(self:Self){unsafe{putchar(.code)}}}
fn write(d:R,n:i32)->i32{d.n=n;return n}
fn apply<F:Fn()->i32>(f:&F)->i32{return f()}
fn apply_mut<F:FnMut()->i32>(f:&mut F)->i32{return f()}
fn once<F:FnOnce()->i32>(f:F)->i32{return f()}
fn owned()->impl Fn()->i32{let r=R{code:65,n:42};return move ()=>r.n}fn local()->i32{let f=owned();let g=()=>f();return g()-42}fn main()->i32=>local();