extern "C" fn putchar(code:i32)->i32;
resource R{code:i32}
impl Drop for R{fn drop(self:Self){unsafe{putchar(.code)}}}
package P{manages owner:R;n:i32}
enum Fault{Bad}
fn fail()->Fault!i32{return Fault.Bad()}
fn reset(target:&mut P)->Fault!R contract{result(create)}{*target=P{owner:R{code:66},n:fail()?};return R{code:67}}
fn work()->i32{var packet=P{owner:R{code:65},n:0};match reset(&mut packet){.ok(extra)=>{return extra.code},.err(_)=>{let old=packet.owner;return old.code-23}}}
fn main()->i32{let seen=work();unsafe{putchar(48+seen/10);putchar(48+seen%10);putchar(10)}return 0}
