resource R{n:i32}
package Leaf{manages a:R;manages b:R}
package Root{child:Leaf}
fn broken(root:Root)->Root{let child=root.child;return root}
fn main(){}
