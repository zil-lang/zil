resource R{n:i32}
package Leaf{manages owner:R;tag:i32}
package Root{child:Leaf;tag:i32}
fn identity(p:Root)->Root{return p}
fn write(p:&mut Leaf){p.tag=7}
fn main(){}
