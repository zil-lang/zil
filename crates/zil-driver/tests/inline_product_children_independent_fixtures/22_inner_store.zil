resource R{n:i32}
package Leaf{manages a:R;manages b:R}
package Root{child:Leaf}
package Stored{manages store:Store<Leaf>}package Forbidden{child:Stored}
fn main(){}
