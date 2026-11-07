resource R{code:i32}
package P{manages owner:R;n:i32}
enum F{Bad}
fn reset(target:&mut P,give incoming:R)->F!i32{*target=P{owner:give incoming,n:42};return 7}
fn incoming(give input:R)->R contract{result(give)}{var packet=P{owner:R{code:65},n:0};match reset(&mut packet,give input){.ok(_)=>{return give packet.owner},.err(_)=>{return give packet.owner}}}
fn main()->i32{let owner=incoming(R{code:66});return owner.code-24}
