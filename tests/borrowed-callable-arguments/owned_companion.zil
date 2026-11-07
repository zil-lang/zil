package Leaf{manages bytes:ByteBuffer}
package Items{manages store:Store<Leaf>}
package Fault{items:Items}
fn keep<F:FnOnce(i32)->i32>(f:F,data:Fault!Items)->F{return f}
fn main(){let store:Store<Leaf>=store_new();let data:Fault!Items=Items{store:give store};let current=keep((n:i32)=>n,data)}
