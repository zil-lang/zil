resource R{n:i32}
package Leaf{manages a:R;manages b:R}
package Root{child:Leaf}
enum E{Fail}package Forbidden{child:Leaf;pending:E!Leaf}
fn main(){}
