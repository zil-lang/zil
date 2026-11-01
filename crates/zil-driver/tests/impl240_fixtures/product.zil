resource R{n:i32}impl Drop for R{fn drop(self:Self){}}
package Leaf{manages r:R}
enum E{Nil,Two(Leaf,Leaf)}
package Root{value:E}
fn make()->void!Root{return Root{value:E.Two(Leaf{r:R{n:65}},Leaf{r:R{n:66}})}}
fn unwrap(root:Root)->Root{var current=root;let value=current.value;current.value=E.Nil;return current}
fn main()->void!i32{var root=make()?;root=unwrap(root);let value=root.value;match value{
 E.Nil()=>{}
 E.Two(a,b)=>{}
 }
root.value=E.Nil;return 0}
