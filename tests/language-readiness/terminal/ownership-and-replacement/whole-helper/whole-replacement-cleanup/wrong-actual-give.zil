resource R{code:i32}
package P{manages owner:R;n:i32}
fn reset(target:&mut P){*target=P{owner:R{code:66},n:42}}
fn wrong(input:P)->R contract{result(give)}{var packet=input;var other=P{owner:R{code:67},n:0};reset(&mut other);return give packet.owner}
fn main()->i32{let owner=wrong(P{owner:R{code:66},n:0});return owner.code-24}
