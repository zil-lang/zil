resource R{n:i32}
package Leaf{manages a:R;manages b:R}
package Root{child:Leaf}
package Forbidden{child:Leaf;items:[Leaf;2]}
fn main(){}
