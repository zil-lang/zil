extern "C" fn putchar(code:i32)->i32;
resource R{code:i32}
package P{manages owner:R;n:i32}
impl Drop for R{fn drop(self:Self){unsafe{putchar(.code)}}}
enum F{Bad}
fn fail()->F!i32{return F.Bad()}
fn assemble(give spare:R,n:i32)->P{return P{owner:R{code:n},n:42}}
fn reset(target:&mut P)->F!i32{*target=assemble(R{code:66},fail()?);return 0}
fn work()->F!i32{var packet=P{owner:R{code:65},n:0};reset(&mut packet)?;return packet.n}
fn main()->i32{match work(){.ok(_)=>{return 0},.err(_)=>{unsafe{putchar(52);putchar(50);putchar(10)}return 42}}}
