resource R{n:i32}
package Leaf{manages a:R;manages b:R}
package Root{child:Leaf}
fn main()->i32{var root=Root{child:Leaf{a:R{n:11},b:R{n:13}}};var child=root.child;let alias=child.a;child.a=R{n:29};root.child=child;return alias.n-11}
