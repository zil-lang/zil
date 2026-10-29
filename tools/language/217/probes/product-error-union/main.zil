resource R{n:i32}
package Leaf{manages owner:R;n:i32}
package Root{child:Leaf;tag:i32}
fn make()->void!Root{return Root{child:Leaf{owner:R{n:1},n:2},tag:3}}
fn main(){}
