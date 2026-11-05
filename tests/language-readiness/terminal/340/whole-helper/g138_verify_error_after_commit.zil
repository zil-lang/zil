resource R{code:i32}
package P{manages owner:R;n:i32}
enum F{Bad}
fn reset(target:&mut P)->F!i32{*target=P{owner:R{code:66},n:42};return F.Bad()}
fn main()->i32{var packet=P{owner:R{code:65},n:0};match reset(&mut packet){.ok(_)=>{return 0},.err(_)=>{let next=&mut packet;let current=next.owner;return next.n+current.code-66}}}
