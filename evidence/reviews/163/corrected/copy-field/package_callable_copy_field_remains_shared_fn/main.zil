extern "C" fn putchar(x:i32)->i32;
fn answer(n:i32)->i32=>n+1
resource R{n:i32}impl Drop for R{fn drop(self:Self){unsafe{putchar(65)}}}
package P{manages r:R;call:fn(i32)->i32}
fn make()->impl Fn()->i32{let p=P{r:R{n:41},call:answer};return move()=>p.call(p.r.n)}
fn main()->i32{let f=make();return f()+f()-84}
