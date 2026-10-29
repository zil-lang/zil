extern "C" fn putchar(x:i32)->i32;
resource R{code:i32}impl Drop for R{fn drop(self:Self){unsafe{putchar(.code)}}}

impl R{fn finish(give self:Self)->i32=>.code}
fn make()->impl FnOnce()->i32{let r=R{code:65};return()=>{let middle=()=>{let inner=move()=>(give r).finish();return inner()};return middle()}}
fn main()->i32{let f=make();return f()-65}
