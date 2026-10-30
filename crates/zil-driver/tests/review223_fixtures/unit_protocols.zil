resource R{n:i32}
package Leaf{manages owner:R;tag:i32}
package Root{child:Leaf;tag:i32}
fn carry(p:Root!void)->Root!void{return p}
fn pop(s:Store<Root>)->void!Root{return store_pop(s)}
fn main(){}
