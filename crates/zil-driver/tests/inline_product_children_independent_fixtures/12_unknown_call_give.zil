resource R{n:i32}
package Leaf{manages a:R;manages b:R}
package Root{child:Leaf}
fn make()->Root{return Root{child:Leaf{a:R{n:11},b:R{n:13}}}}
fn relay(value:Root)->Root{return value}
fn origin()->R contract{result(give)}{let root=relay(make());let child=root.child;return give child.a}
fn main(){}
