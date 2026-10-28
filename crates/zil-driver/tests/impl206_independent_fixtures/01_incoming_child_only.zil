resource R{n:i32}
package Leaf{manages a:R;manages b:R}
package Only{child:Leaf}
package Shell{inner:Only}
fn relay(v:Shell)->Shell{return v}
fn extract(v:Shell)->R contract{result(give)}{let mid=v.inner;let leaf=mid.child;return give leaf.a}
fn main()->i32{let value=extract(relay(Shell{inner:Only{child:Leaf{a:R{n:11},b:R{n:13}}}}));return value.n-11}
