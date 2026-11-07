resource R{n:i32}
package Leaf{manages a:R;manages b:R}
package Root{child:Leaf}
fn choose(give old:R,flag:bool)->R contract{result(create)}{let root=if flag{Root{child:Leaf{a:R{n:29},b:give old}}}else{Root{child:Leaf{a:give old,b:R{n:29}}}};let child=root.child;return give child.a}
fn main()->i32{let a=choose(R{n:17},true);let b=choose(R{n:19},false);return a.n+b.n-58}
