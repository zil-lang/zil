resource R{code:i32}
package P{manages owner:R;n:i32}
fn reset(target:&mut P,give incoming:R){*target=P{owner:give incoming,n:42}}
fn fresh(give input:R)->R contract{result(create)}{var packet=P{owner:R{code:65},n:0};reset(&mut packet,give input);return give packet.owner}
fn main()->i32{let next=fresh(R{code:66});return next.code}
