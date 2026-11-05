resource R{code:i32}
package P{manages owner:R;n:i32}
enum F{Bad}
fn reset(target:&mut P)->F!i32{return F.Bad()}
fn main()->i32{var packet=P{owner:R{code:42},n:0};let old=packet.owner;match reset(&mut packet){.ok(_)=>{return 0},.err(_)=>{return old.code}}}
