extern "C" fn putchar(code:i32)->i32;
resource R{code:i32}
impl Drop for R{fn drop(self:Self){unsafe{putchar(.code)}}}
package P{manages owner:R;n:i32}
fn modify(target:&mut P)->i32{target.n+=1;return 42}
fn reset(target:&mut P,other:&mut P){*target=P{owner:R{code:66},n:modify(other)}}
fn work()->i32{var packet=P{owner:R{code:65},n:0};var other=P{owner:R{code:67},n:1};reset(&mut packet,&mut other);let next=&mut packet;let current=next.owner;return next.n+current.code-66}
fn main()->i32{let seen=work();unsafe{putchar(48+seen/10);putchar(48+seen%10);putchar(10)}return 0}
