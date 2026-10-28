resource R{n:i32}
package Leaf{manages a:R;manages b:R}
package Root{child:Leaf}
fn main(){var root=Root{child:Leaf{a:R{n:11},b:R{n:13}}};let child=root.child;root.child=child;root.child=child}
