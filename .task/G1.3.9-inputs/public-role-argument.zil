extern "C" fn putchar(code:i32)->i32;
resource R{code:i32}
impl Drop for R{fn drop(self:Self){unsafe{putchar(.code)}}}
package P{manages owner:R;n:i32}
fn reset(target:&mut P){*target=P{owner:R{code:66},n:42}}
fn run<F:FnMut(&mut P)->void>(f:&mut F,target:&mut P){f(target)}
fn work()->i32{var packet=P{owner:R{code:65},n:0};var update=(target:&mut P)->void=>{*target=P{owner:R{code:66},n:42}};run(&mut update,&mut packet);let next=&mut packet;let current=next.owner;return next.n+current.code-66}
fn main()->i32{let seen=work();unsafe{putchar(48+seen/10);putchar(48+seen%10);putchar(10)}return 0}
