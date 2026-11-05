extern "C" fn putchar(code:i32)->i32;
resource R{code:i32}
impl Drop for R{fn drop(self:Self){unsafe{putchar(.code)}}}
package P{manages owner:R;n:i32}
enum Fault{Bad}
fn fail()->Fault!i32{return Fault.Bad()}
fn reset(target:&mut (P,i32))->Fault!i32{*target=(P{owner:R{code:66},n:fail()?},0);return 0}
fn work()->i32{var packet=(P{owner:R{code:65},n:42},0);match reset(&mut packet){.ok(n)=>{return n},.err(_)=>{let(ref old,_)=packet;return old.n}}}
fn main()->i32{let seen=work();unsafe{putchar(48+seen/10);putchar(48+seen%10);putchar(10)}return 0}
