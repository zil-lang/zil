extern "C" fn putchar(code:i32)->i32;
resource R{code:i32}
impl Drop for R{fn drop(self:Self){unsafe{putchar(.code)}}}
package P{manages owner:R;n:i32}
fn incoming(give input:R)->R contract{result(create)}{
    var packet=P{owner:R{code:65},n:0};
    let target=&mut packet;
    *target=P{owner:give input,n:42};
    return give packet.owner
}
fn main()->i32{let owner=incoming(R{code:66});return owner.code-24}
