extern "C" fn putchar(code:i32)->i32;
resource R{code:i32}
impl Drop for R{fn drop(self:Self){unsafe{putchar(.code)}}}
package P{manages owner:R;n:i32}
enum Fault{Bad}
fn make()->Fault!P{return P{owner:R{code:65},n:42}}
fn fail()->Fault!i32{return Fault.Bad()}
fn stage(give owner:R,n:i32)->Fault!P{return P{owner:give owner,n:n}}
fn reset(target:&mut (Fault!P))->Fault!i32{*target=stage(R{code:66},fail()?);return 0}
fn work()->i32{var packet=make();match reset(&mut packet){.ok(n)=>{return n},.err(_)=>{match packet{.ok(previous)=>{return previous.n},.err(_)=>{return 0}}}}}
fn main()->i32{let seen=work();unsafe{putchar(48+seen/10);putchar(48+seen%10);putchar(10)}return 0}
