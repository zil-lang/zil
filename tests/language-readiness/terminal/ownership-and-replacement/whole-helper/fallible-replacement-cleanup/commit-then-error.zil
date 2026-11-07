extern "C" fn putchar(code:i32)->i32;
resource R{code:i32}
package P{manages owner:R;n:i32}
impl Drop for R{fn drop(self:Self){unsafe{putchar(.code)}}}
enum F{Bad}
fn reset(target:&mut P)->F!i32{*target=P{owner:R{code:66},n:42};return F.Bad()}
fn work()->i32{var packet=P{owner:R{code:65},n:0};match reset(&mut packet){.ok(_)=>{return 0},.err(_)=>{let next=&mut packet;let current=next.owner;return next.n+current.code-66}}}
fn show(n:i32){unsafe{putchar(48+n/10);putchar(48+n%10);putchar(10)}}
fn main()->i32{show(work());return 0}
