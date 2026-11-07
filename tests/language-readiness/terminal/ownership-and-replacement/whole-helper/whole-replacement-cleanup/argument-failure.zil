extern "C" fn putchar(code:i32)->i32;
resource R{code:i32}
package P{manages owner:R;n:i32}
impl Drop for R{fn drop(self:Self){unsafe{putchar(.code)}}}
enum F{Bad}
fn fail()->F!i32{return F.Bad()}
fn reset(target:&mut P,give incoming:R,n:i32){*target=P{owner:give incoming,n:n}}
fn work()->F!i32{var packet=P{owner:R{code:65},n:0};reset(&mut packet,R{code:66},fail()?);return packet.n}
fn show(n:i32){unsafe{putchar(48+n/10);putchar(48+n%10);putchar(10)}}
fn main()->i32{match work(){.ok(n)=>{show(n);return 0},.err(_)=>{show(42);return 0}}}
