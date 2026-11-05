extern "C" fn putchar(code:i32)->i32;
resource R{code:i32}
impl Drop for R{fn drop(self:Self){unsafe{putchar(.code)}}}
package P{manages owner:R;n:i32}
fn reset(target:&mut (P,i32)){*target=(P{owner:R{code:66},n:42},0)}
fn work()->i32{var packet=(P{owner:R{code:65},n:0},0);let (previous,ref count)=packet;reset(&mut packet);let (ref current,n)=packet;let owner=current.owner;return previous.n}
fn main()->i32{let seen=work();unsafe{putchar(48+seen/10);putchar(48+seen%10);putchar(10)}return 0}
