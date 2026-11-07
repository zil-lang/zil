resource R{n:i32}
package Leaf{manages a:R;manages b:R}
package Root{child:Leaf}
fn regenerate(input:Root)->R contract{result(create)}{var root=input;var child=root.child;child.a=R{n:29};root.child=child;let transferred=root;let leaf=transferred.child;return give leaf.a}
fn main()->i32{let value=regenerate(Root{child:Leaf{a:R{n:11},b:R{n:13}}});return value.n-29}
