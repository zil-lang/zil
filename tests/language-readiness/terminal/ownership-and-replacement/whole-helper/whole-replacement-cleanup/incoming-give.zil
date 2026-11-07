resource R{code:i32}
package P{manages owner:R;n:i32}
fn reset(target:&mut P,give incoming:R){*target=P{owner:give incoming,n:42}}
fn incoming(give input:R)->R contract{result(give)}{var packet=P{owner:R{code:65},n:0};reset(&mut packet,give input);return give packet.owner}
fn main()->i32{let owner=incoming(R{code:66});return owner.code-24}
