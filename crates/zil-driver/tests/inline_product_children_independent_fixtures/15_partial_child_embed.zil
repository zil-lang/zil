resource R{n:i32}
package Leaf{manages a:R;manages b:R}
package Root{child:Leaf}
fn main(){var child=Leaf{a:R{n:11},b:R{n:13}};let moved=give child.a;let root=Root{child:child}}
