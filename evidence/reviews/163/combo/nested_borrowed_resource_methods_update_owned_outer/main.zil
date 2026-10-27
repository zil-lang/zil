extern "C" fn putchar(x:i32)->i32;
resource R{code:i32}impl Drop for R{fn drop(self:Self){unsafe{putchar(.code)}}}

impl R(self:Self){fn add(n:i32){.code+=n};fn read()->i32=>.code}
fn make()->impl FnMut(i32)->i32{let r=R{code:40};return move(n:i32)=>{let inner=(delta:i32)=>{r.add(delta);return r.read()};return inner(n)}}
fn main()->i32{var f=make();if f(1)!=41||f(1)!=42{return 1};return 0}
