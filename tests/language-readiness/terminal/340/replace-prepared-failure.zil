extern "C" fn putchar(code:i32)->i32;
resource R{code:i32}
impl Drop for R{fn drop(self:Self){unsafe{putchar(.code)}}}
package P{manages owner:R;n:i32}
enum F{Bad}
fn fail()->F!i32{return F.Bad()}
fn make(give spare:R,n:i32)->R contract{result(create)}{return R{code:n}}
fn replace(target:&mut P)->F!i32{
    target.owner=make(R{code:66},fail()?);
    return 0
}
fn result()->i32{
    var packet=P{owner:R{code:65},n:0};
    match replace(&mut packet){
        .ok(_)=>{return 0},
        .err(_)=>{return packet.owner.code-23}
    }
}
