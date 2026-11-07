resource R{code:i32}
package P{manages owner:R;n:i32}
fn reset(target:&mut P)->i32{*target=P{owner:R{code:66},n:42};return 7}
fn fresh(input:P)->R contract{result(give)}{var packet=input;let returned=reset(&mut packet);return give packet.owner}
fn main()->i32{let owner=fresh(P{owner:R{code:65},n:0});return owner.code-24}
