resource R{n:i32}
package Leaf{manages a:R;manages b:R}
package Root{child:Leaf}

fn main(){let root=Root{child:Leaf{a:R{n:11},b:R{n:13}}};let f=move()=>{let child=root.child;return child.a.n};f()}
