extern "C" fn putchar(code:i32)->i32;
resource R{code:i32}
impl Drop for R{fn drop(self:Self){unsafe{putchar(.code)}}}
package P{manages owner:R;n:i32}
fn fail()->P!i32{return P{owner:R{code:67},n:42}}
fn reset(target:&mut P)->P!i32{*target=P{owner:R{code:66},n:fail()?};return 0}
fn work()->i32{var packet=P{owner:R{code:65},n:0};match reset(&mut packet){.ok(n)=>{return n},.err(error)=>{let old=packet.owner;return error.n+old.code-65}}}
fn main()->i32{let seen=work();unsafe{putchar(48+seen/10);putchar(48+seen%10);putchar(10)}return 0}
