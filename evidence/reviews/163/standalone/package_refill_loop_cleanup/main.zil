extern "C" fn putchar(x:i32)->i32;
resource R{code:i32}impl Drop for R{fn drop(self:Self){unsafe{putchar(.code)}}}
package P{manages a:R;manages b:R;n:i32}
impl P{fn finish(self:Self)->i32=>.n}
fn make()->impl FnMut()->i32{var p=P{a:R{code:65},b:R{code:66},n:1};return move()=>{let n=p.finish();p=P{a:R{code:67},b:R{code:68},n:n+1};return n}}
fn main()->i32{var f=make();var sum=0;for i in 0..2{sum+=f()};return sum-3}
