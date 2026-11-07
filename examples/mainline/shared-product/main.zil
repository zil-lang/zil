extern "C" fn putchar(c:i32)->i32;
fn emit(c:i32){unsafe{putchar(c)}}
resource R{n:i32}
impl Drop for R{fn drop(self:Self){emit(.n)}}
package Leaf{manages owner:R;value:i32}
package Root{child:Leaf;manages tail:R;tag:i32}
fn value(p:&Root)->i32{return p.child.value+p.child.owner.n+p.tail.n+p.tag}
fn main()->i32{
    let root=Root{child:Leaf{owner:R{n:65},value:7},tail:R{n:66},tag:11}
    let p=&root
    let q=p
    if value(q)!=149{return 3}
    let child=root.child
    emit(88)
    return child.value-7
}
