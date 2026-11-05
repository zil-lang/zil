extern "C" fn putchar(code:i32)->i32;
resource R{code:i32}
package P{manages owner:R;n:i32}
impl Drop for R{fn drop(self:Self){unsafe{putchar(.code)}}}
enum F{Bad}
fn fail()->F!i32{return F.Bad()}
fn assemble(give spare:R,n:i32)->P{return P{owner:R{code:n},n:42}}
fn reset(target:&mut P)->F!i32{*target=assemble(R{code:66},fail()?);return 0}
fn work()->i32{var packet=P{owner:R{code:65},n:0};match reset(&mut packet){.ok(_)=>{return 0},.err(_)=>{let next=&mut packet;let current=next.owner;return next.n+current.code-23}}}
fn main()->i32{let seen=work();unsafe{putchar(48+seen/10);putchar(48+seen%10);putchar(10)}return 0}
