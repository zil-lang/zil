
extern "C" fn putchar(x:i32)->i32;
resource R{code:i32} impl Drop for R{fn drop(self:Self){unsafe{putchar(.code)}}}
package Item{manages r:R;code:i32}
package Batch{manages items:Store<Item>;manages bytes:ByteBuffer}
package State<F>{manages items:Store<Item>;callback:F;hits:i32;manages bytes:ByteBuffer}
fn item(code:i32)->Item=>Item{r:R{code:code},code:code}
fn empty()->Store<Item> contract{result(create)}{return store_new()}
fn batch()->Batch{return Batch{items:empty(),bytes:bytes_new()}}
fn make(code:i32)->impl FnMut()->i32{let guard=R{code:code};var n=0;return move()=>{n+=1;return n+guard.code-code}}
fn pack<F:FnMut()->i32>(callback:F)->State<F>{let b=batch();return State::<F>{items:give b.items,callback:callback,hits:0,bytes:give b.bytes}}
fn read(value:&Item)->i32=>value.code
fn consume<F:FnMut()->i32>(value:State<F>){}
 fn update<G:FnMut()->i32>(state:State<G>)->State<G>{var next=state;let cb=&mut next.callback;next.hits=10;next.bytes=bytes_new();next.hits+=cb();next.hits+=cb();return next}fn main()->i32{let state=update(pack(make(65)));return state.hits-13}