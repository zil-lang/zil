
extern "C" fn putchar(n:i32)->i32;
resource R{code:i32}
impl Drop for R{fn drop(self:Self){unsafe{putchar(.code)}}}
fn take(give r:R){}fn apply<T:Copy,F:FnOnce(T)->T>(f:F,x:T)->T=>f(x);fn main()->i32{let r=R{code:65};let f=move(n:i32)=>{take(give r);return n};return apply(f,7)-7}