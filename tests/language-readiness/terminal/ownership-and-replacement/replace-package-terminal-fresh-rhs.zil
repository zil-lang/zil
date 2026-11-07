extern "C" fn putchar(code:i32)->i32;
resource R{code:i32}
impl Drop for R{fn drop(self:Self){unsafe{putchar(.code)}}}
package P{manages owner:R;n:i32}
fn fresh(input:P)->R contract{result(create)}{
    var packet=input;
    let target=&mut packet;
    *target=P{owner:R{code:66},n:42};
    return give packet.owner
}
fn main()->i32{let owner=fresh(P{owner:R{code:65},n:0});return owner.code-24}
