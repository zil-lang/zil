extern "C" fn putchar(n:i32)->i32;
resource R{code:i32}
impl Drop for R{fn drop(self:Self){unsafe{putchar(.code);}}}
package P{manages owner:R;n:i32}
fn reset(target:&mut P){target.owner=R{code:66};}
fn main()->i32{
    var packet=P{owner:R{code:65},n:0};
    let old=packet.owner;
    reset(&mut packet);
    return old.code;
}
