resource R{code:i32}
package P{manages owner:R;n:i32}
fn reset(target:&mut P,give incoming:R){*target=P{owner:give incoming,n:42}}
fn main()->i32{var packet=P{owner:R{code:65},n:0};reset(&mut packet,R{code:66});let next=&mut packet;let current=next.owner;return next.n+current.code-66}
