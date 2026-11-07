resource R{code:i32}
package P{manages owner:R;n:i32}
fn reset(target:&mut P){target.owner=R{code:66};target.n=42}
fn peek(input:P)->i32{
    var packet=input;
    reset(&mut packet);
    let next=&mut packet;
    let current=next.owner;
    return next.n+current.code-66
}
fn main()->i32{return peek(P{owner:R{code:65},n:0})}
