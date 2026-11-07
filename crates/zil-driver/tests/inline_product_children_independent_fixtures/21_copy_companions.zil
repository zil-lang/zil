resource R{n:i32}
package Leaf{manages a:R;manages b:R}
package Root{child:Leaf}
enum E{A(i32),B}
fn plus(n:i32)->i32{return n+1}
package WithCopy{child:Leaf;tag:E;status:E!i32;pointer:fn(i32)->i32;callback:extern "C" fn(i32)->i32}
fn main()->E!i32{let status:E!i32=7;let root=WithCopy{child:Leaf{a:R{n:11},b:R{n:13}},tag:E.A(3),status:status,pointer:plus,callback:c_callback(plus)};let moved=root;let result=moved.status?;let pointer=moved.pointer;let tag=moved.tag;let child=moved.child;match tag{E.A(n)=>{return child.a.n+result+pointer(4)+n-26},E.B()=>{return 1}}}
