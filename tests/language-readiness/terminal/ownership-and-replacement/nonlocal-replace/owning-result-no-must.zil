extern "C" fn putchar(code:i32)->i32;
resource R{code:i32}
impl Drop for R{fn drop(self:Self){unsafe{putchar(.code)}}}
package P{manages owner:R;n:i32}
fn reset(target:&mut P)->R contract{result(create)}{*target=P{owner:R{code:66},n:42};return R{code:67}}
fn wrap(input:P)->R contract{result(create)}{var packet=input;let extra=reset(&mut packet);return give packet.owner}
fn main()->i32{let owner=wrap(P{owner:R{code:65},n:0});return owner.code}
