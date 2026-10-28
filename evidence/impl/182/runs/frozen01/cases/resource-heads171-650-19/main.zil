
extern "C" fn putchar(x:i32)->i32;
resource R{code:i32;n:i32}
impl Drop for R{fn drop(self:Self){unsafe{putchar(.code)}}}
impl R{fn peek(self:&Self)->i32=>.n;fn consume(give self:Self)->i32=>.n}
fn read(r:&R)->i32=>r.n
fn write(r:R,n:i32){r.n=n}
fn helper(d:R){write(d,43)}fn effect(d:R,h:&R)->i32{helper(d);return h.n}fn main()->i32{let r=R{code:65,n:42};let other=R{code:66,n:42};return effect(r,&other)-42}