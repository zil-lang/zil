resource R{code:i32}
package P{manages owner:R;n:i32}
fn main()->i32{
    var packet=P{owner:R{code:65},n:0};
    let target=&mut packet;
    *target=P{owner:R{code:66},n:42};
    let next=&mut packet;
    let current=next.owner;
    return next.n+current.code-66
}
