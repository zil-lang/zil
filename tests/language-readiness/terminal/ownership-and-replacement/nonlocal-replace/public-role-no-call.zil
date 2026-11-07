extern "C" fn putchar(code:i32)->i32;
resource R{code:i32}
impl Drop for R{fn drop(self:Self){unsafe{putchar(.code)}}}
package P{manages owner:R;n:i32}
fn reset(target:&mut P){*target=P{owner:R{code:66},n:42}}
fn run<F:FnMut()->void>(f:&mut F){}
fn work()->i32{var packet=P{owner:R{code:65},n:0};let old=packet.owner;let target=&mut packet;var update=()->void=>{*target=P{owner:R{code:66},n:42}};run(&mut update);let next=&mut packet;let current=next.owner;return next.n+old.code-65+42}
fn main()->i32{let seen=work();unsafe{putchar(48+seen/10);putchar(48+seen%10);putchar(10)}return 0}
