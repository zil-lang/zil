resource R{n:i32}
package Leaf{manages owner:R;value:i32}
package Root{child:Leaf;tag:i32}
fn read(p:&Root)->i32{return p.child.owner.n+p.child.value+p.tag}
fn main()->i32{
    let root=Root{child:Leaf{owner:R{n:5},value:7},tag:11}
    let p=&root
    let moved=root
    return read(p)
}
