resource R{code:i32}
package P{manages owner:R;n:i32}
fn reset(target:&mut P,replacement:P){*target=replacement}
fn incoming(input:P)->R contract{result(give)}{var packet=P{owner:R{code:65},n:0};reset(&mut packet,input);return give packet.owner}
fn main()->i32{let owner=incoming(P{owner:R{code:66},n:42});return owner.code-24}
