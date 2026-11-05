extern "C" fn putchar(code:i32)->i32;
resource R{code:i32}
impl Drop for R{fn drop(self:Self){unsafe{putchar(.code)}}}
package Child{manages owner:R;n:i32}
package P{child:Child;manages spare:R;n:i32}
fn reset(target:&mut P){*target=P{child:Child{owner:R{code:67},n:20},spare:R{code:68},n:22}}
fn work()->i32{
    var packet=P{child:Child{owner:R{code:65},n:0},spare:R{code:66},n:0};
    let child=packet.child;reset(&mut packet);
    return packet.child.n+packet.n;
}
fn main()->i32{let n=work();unsafe{putchar(48+n/10);putchar(48+n%10);putchar(10)}return 0}
