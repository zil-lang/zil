resource R{n:i32}
package Leaf{manages a:R;manages b:R}
package Root{child:Leaf}
enum E{Fail}
fn unused(value:E!Root){}fn main(){}
