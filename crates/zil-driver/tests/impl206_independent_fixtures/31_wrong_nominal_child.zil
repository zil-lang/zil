resource R{n:i32}
package Leaf{manages a:R;manages b:R}
package Root{child:Leaf}
package Lookalike{manages a:R;manages b:R}
fn main(){let child=Lookalike{a:R{n:11},b:R{n:13}};let root=Root{child:child}}
