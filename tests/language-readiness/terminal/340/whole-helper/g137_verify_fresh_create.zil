resource R{code:i32}
package P{manages owner:R;n:i32}
fn reset(target:&mut P){*target=P{owner:R{code:66},n:42}}
fn fresh(input:P)->R contract{result(create)}{var packet=input;reset(&mut packet);return give packet.owner}
fn main()->i32{let next=fresh(P{owner:R{code:65},n:0});return next.code-24}
