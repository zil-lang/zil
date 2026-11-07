extern "C" fn putchar(n:i32)->i32;
resource Mark{code:i32}
impl Drop for Mark{fn drop(self:Self){unsafe{putchar(.code)}}}
package Leaf{manages mark:Mark;manages bytes:ByteBuffer}
package Set{manages items:Store<Leaf>;ready:bool}
package Session{left:Set;right:Set;ready:bool}
fn empty()->Set{return Set{items:store_new(),ready:false}}
fn make(code:i32,size:i64)->Set{
 let items:Store<Leaf>=store_new()
 if !store_reserve(items,1){return Set{items:give items,ready:false}}
 let mark=Mark{code:code}
 let bytes=bytes_new()
 if !bytes_reserve(bytes,size){return Set{items:give items,ready:false}}
 store_push(items,Leaf{mark:give mark,bytes:give bytes})
 return Set{items:give items,ready:true}
}
fn session()->Session{
 let a=make(65,7)
 if !a.ready{return Session{left:a,right:empty(),ready:false}}
 let b=make(66,11)
 let ready=b.ready
 return Session{left:a,right:b,ready:ready}
}
fn main()->void!i32{
 var s=session()
 if !s.ready{return 1}
 let replacement=make(67,13)
 if !replacement.ready{return 1}
 s.left=replacement
 var right=s.right
 if !store_reserve(right.items,2){return 1}
 let popped=store_pop(right.items)?
 return 0
}
