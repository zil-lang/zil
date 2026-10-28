
extern "C" fn putchar(x:i32)->i32;
resource R{code:i32;n:i32}
impl Drop for R{fn drop(self:Self){unsafe{putchar(.code)}}}
impl R{fn peek(self:&Self)->i32=>.n;fn consume(give self:Self)->i32=>.n}
fn read(r:&R)->i32=>r.n
fn write(r:R,n:i32){r.n=n}
fn apply<F:Fn()->i32>(f:&F)->i32{return f()}fn main()->i32{let r=R{code:65,n:42};let d=r;let p=&r;let field=&p.n;let f=()=>{write(d,43);return *field};return apply(&f)}