extern "C" fn putchar(code:i32)->i32;
resource R{code:i32}
impl Drop for R{fn drop(self:Self){unsafe{putchar(.code)}}}
package P{manages owner:R;n:i32}
fn reset(target:&mut P)->R contract{result(create)}{*target=P{owner:R{code:66},n:42};return R{code:67}}
fn work()->i32{var packet=P{owner:R{code:65},n:0};let extra=reset(&mut packet);let next=&mut packet;let current=next.owner;return next.n+current.code+extra.code-133}
fn main()->i32{let seen=work();unsafe{putchar(48+seen/10);putchar(48+seen%10);putchar(10)}return 0}
