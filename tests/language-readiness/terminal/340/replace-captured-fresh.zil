extern "C" fn putchar(code:i32)->i32;
resource R{code:i32}
impl Drop for R{fn drop(self:Self){unsafe{putchar(.code)}}}
package P{manages owner:R;n:i32}
fn make()->R contract{result(create)}{
    var pair=(P{owner:R{code:65},n:0},0);
    let target=&mut pair;
    var replace=()->void=>{let(ref mut packet,_)=*target;packet.owner=R{code:66}};
    replace();
    let(packet,_)=pair;
    return give packet.owner
}
fn result()->i32{let owner=make();return owner.code-24}
