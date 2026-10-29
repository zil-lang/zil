extern "C" fn putchar(x:i32)->i32;
resource R{code:i32}impl Drop for R{fn drop(self:Self){unsafe{putchar(.code)}}}

enum E{Fail}impl R{fn finish(give self:Self,give other:R,n:i32)->i32=>.code+other.code+n}
fn fail()->E!i32=>E.Fail
fn make()->impl FnOnce()->E!i32{let a=R{code:65};return move()=>{let b=R{code:66};let inner=move()->E!i32=>{let c=R{code:67};return (give a).finish(give b,fail()?)};return inner()?}}
fn main()->E!i32{let f=make();return f()?}
