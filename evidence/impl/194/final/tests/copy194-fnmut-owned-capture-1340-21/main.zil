
extern "C" fn putchar(n:i32)->i32;
resource R{code:i32}
impl Drop for R{fn drop(self:Self){unsafe{putchar(.code)}}}
fn apply<T:Copy,F:FnMut(T)->T>(f:&mut F,x:T)->T=>f(x);fn main()->i32{let r=R{code:65};var total=0;var f=move(n:i32)=>{r.code;total+=n;return total};return apply(&mut f,3)+apply(&mut f,4)-10}