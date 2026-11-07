extern "C" fn putchar(code:i32)->i32;
resource R{code:i32}
impl Drop for R{fn drop(self:Self){unsafe{putchar(.code)}}}
package P{manages owner:R;n:i32}
fn replace(target:&mut P){target.owner=R{code:66}}
fn relay(target:&mut P){replace(target)}
fn make()->R contract{result(create)}{
    var packet=P{owner:R{code:65},n:0};
    relay(&mut packet);
    return give packet.owner
}
fn result()->i32{let owner=make();return owner.code-24}
