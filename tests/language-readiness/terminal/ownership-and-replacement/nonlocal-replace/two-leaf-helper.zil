extern "C" fn putchar(code:i32)->i32;
resource R{code:i32}
impl Drop for R{fn drop(self:Self){unsafe{putchar(.code)}}}
package P{manages first:R;manages second:R;n:i32}
fn reset(target:&mut P){*target=P{first:R{code:67},second:R{code:68},n:42}}
fn work()->i32{
    var packet=P{first:R{code:65},second:R{code:66},n:0};
    reset(&mut packet);
    return packet.n;
}
fn main()->i32{let n=work();unsafe{putchar(48+n/10);putchar(48+n%10);putchar(10)}return 0}
