resource R{n:i32}
package Leaf{manages a:R;manages b:R}
package Root{child:Leaf}
fn fresh()->R contract{result(create)}{return R{n:29}}
fn origin()->R contract{result(create)}{let root=Root{child:Leaf{a:fresh(),b:R{n:13}}};let child=root.child;return give child.a}
fn main()->i32{let value=origin();return value.n-29}
