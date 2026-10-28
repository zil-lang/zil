resource R{n:i32}
package Leaf{manages a:R;manages b:R}
package Root{child:Leaf}
fn origin(input:Root,count:i32)->R contract{result(create)}{var root=input;var i=0;while i<count{var child=root.child;child.a=R{n:29};root.child=child;i+=1}let final=root.child;return give final.a}
fn main(){}
