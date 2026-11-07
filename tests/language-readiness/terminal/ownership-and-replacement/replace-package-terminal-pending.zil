extern "C" fn putchar(code:i32)->i32;
resource R{code:i32}
impl Drop for R{fn drop(self:Self){unsafe{putchar(.code)}}}
package P{manages owner:R;n:i32}
fn main()->i32{
    var packet=P{owner:R{code:65},n:0};
    let target=&mut packet;
    *target=P{owner:R{code:66},n:42};
    return packet.n+packet.owner.code-66
}
