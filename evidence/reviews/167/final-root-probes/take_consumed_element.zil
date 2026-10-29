extern "C" fn putchar(x:i32)->i32;
resource R{code:i32} impl Drop for R{fn drop(self:Self){unsafe{putchar(.code)}}}
fn make(n:i32)->impl FnOnce()->i32{let r=R{code:n};return move()=>{let own=give r;return own.code}}
enum E{Fail} fn fail()->E!i32{return E.Fail}
fn main()->i32{let a=[make(65),make(66)];let first=a[0];let n=first();let again=a[0];return again()-n}
