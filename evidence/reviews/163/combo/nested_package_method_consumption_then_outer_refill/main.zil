extern "C" fn putchar(x:i32)->i32;
resource R{code:i32}impl Drop for R{fn drop(self:Self){unsafe{putchar(.code)}}}

package P{manages a:R;n:i32}impl P{fn finish(self:Self)->i32=>.n}
fn make()->impl FnMut()->i32{var p=P{a:R{code:65},n:1};return move()=>{let inner=move()=>p.finish();let n=inner();p=P{a:R{code:66},n:n+1};return n}}
fn main()->i32{var f=make();return f()+f()-3}
