resource R{code:i32}
package P{manages owner:R;n:i32}
fn reset(target:&mut P){*target=P{owner:R{code:66},n:42}}
fn main()->i32{var packet=P{owner:R{code:65},n:0};let old=packet.owner;reset(&mut packet);return old.code}
