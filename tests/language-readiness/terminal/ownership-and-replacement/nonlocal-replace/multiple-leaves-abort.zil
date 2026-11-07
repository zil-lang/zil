extern "C" fn putchar(code:i32)->i32;
resource R{code:i32}
impl Drop for R{fn drop(self:Self){unsafe{putchar(.code)}}}
package P{manages first:R;manages second:R;n:i32}
enum F{Bad}
fn fail()->F!i32{return F.Bad()}
fn assemble(give first:R,n:i32)->P{return P{first:give first,second:R{code:68},n:n}}
fn reset(target:&mut P)->F!i32{*target=assemble(R{code:67},fail()?);return 0}
fn work()->i32{
    var packet=P{first:R{code:65},second:R{code:66},n:0};
    match reset(&mut packet){.ok(_)=>{},.err(_)=>{}}
    return packet.first.code-23;
}
fn main()->i32{let n=work();unsafe{putchar(48+n/10);putchar(48+n%10);putchar(10)}return 0}
