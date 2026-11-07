resource R{n:i32}
package Leaf{manages a:R;manages b:R}
package Root{child:Leaf}
enum E{Fail}
fn callback()->impl Fn()->i32{return move()=>7}
fn sink(value:Root){}
fn main(){let value=Root{child:Leaf{a:R{n:11},b:R{n:13}}};sink(value)}
