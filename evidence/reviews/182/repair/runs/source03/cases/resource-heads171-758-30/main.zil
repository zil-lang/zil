
extern "C" fn putchar(x:i32)->i32;
resource R{code:i32;n:i32}
impl Drop for R{fn drop(self:Self){unsafe{putchar(.code)}}}
impl R{fn peek(self:&Self)->i32=>.n;fn consume(give self:Self)->i32=>.n}
fn read(r:&R)->i32=>r.n
fn write(r:R,n:i32){r.n=n}
enum E{Fail} impl R{fn combine(self:&Self,value:i32)->i32=>.n+value} fn fail()->E!i32=>E.Fail;fn main()->E!i32{let r=R{code:65,n:42};return r.combine(fail()?)}