resource R{code:i32}
package P{manages owner:R;n:i32}
fn reset(flag:bool,target:&mut P){if flag{*target=P{owner:R{code:66},n:42}}}
fn main()->i32{var packet=P{owner:R{code:65},n:42};reset(false,&mut packet);let next=&mut packet;let owner=next.owner;return next.n+owner.code-65}
