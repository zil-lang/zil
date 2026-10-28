
extern "C" fn putchar(x:i32)->i32;
resource R{code:i32;n:i32}
impl Drop for R{fn drop(self:Self){unsafe{putchar(.code)}}}
fn write(d:R,n:i32)->i32{d.n=n;return n}
fn apply<F:Fn()->i32>(f:&F)->i32{return f()}
fn apply_mut<F:FnMut()->i32>(f:&mut F)->i32{return f()}
fn once<F:FnOnce()->i32>(f:F)->i32{return f()}
enum E{Fail}fn fail()->E!i32=>E.Fail;fn invoke<F:FnOnce()->i32>(f:F,n:i32)->i32{return f()+n}fn main()->E!i32{let r=R{code:65,n:1};let d=r;let token=R{code:66,n:1};let n=40;let q=&n;let f=move()=>{unsafe{putchar(88)};d.n=1;return token.n+d.n+*q};return invoke(f,fail()?);}