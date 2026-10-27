extern "C" fn putchar(x:i32)->i32;
resource R{code:i32} impl Drop for R{fn drop(self:Self){unsafe{putchar(.code)}}}
fn make(n:i32)->impl FnOnce()->i32{let r=R{code:n};return move()=>{let own=give r;return own.code}}
enum E{Fail} fn fail()->E!i32{return E.Fail}
fn main()->i32{var a=[make(65),make(66),make(67)];let later=R{code:90};let old=a[1];a[1]=make(68);if old()!=66{return 1};let b=a;let tail=b[2];if tail()!=67{return 2};return later.code-90}
