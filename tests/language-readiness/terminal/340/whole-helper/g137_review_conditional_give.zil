resource R{code:i32}
package P{manages owner:R;n:i32}
fn reset(target:&mut P,change:bool){if change{*target=P{owner:R{code:66},n:42}}}
fn conditional(input:P,change:bool)->R contract{result(give)}{var packet=input;reset(&mut packet,change);return give packet.owner}
fn main()->i32{let owner=conditional(P{owner:R{code:66},n:0},false);return owner.code-24}
