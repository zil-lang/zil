
extern "C" fn putchar(x:i32)->i32;
resource R{code:i32;n:i32}
impl Drop for R{fn drop(self:Self){unsafe{putchar(.code)}}}
fn write(d:R,n:i32)->i32{d.n=n;return n}
fn apply<F:Fn()->i32>(f:&F)->i32{return f()}
fn apply_mut<F:FnMut()->i32>(f:&mut F)->i32{return f()}
fn once<F:FnOnce()->i32>(f:F)->i32{return f()}
fn main()->i32{let r=R{code:65,n:7};let d=r;let p=&r;let q=&p.n;let f=()=>d.n+*q;var total=0;for i in 0..3{total+=apply(&f)};return total-42}