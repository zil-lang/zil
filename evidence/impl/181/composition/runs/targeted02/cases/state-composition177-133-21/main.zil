
extern "C" fn putchar(x:i32)->i32;
resource R{code:i32} impl Drop for R{fn drop(self:Self){unsafe{putchar(.code)}}}
package Item{manages r:R}
package Batch{manages items:Store<Item>}
package State<F>{manages r:R;manages items:Store<Item>;callback:F;hits:i32}
fn batch()->Batch{let items:Store<Item>=store_new();store_reserve(items,1);store_push(items,Item{r:R{code:65}});return Batch{items:give items}}
fn consume(b:Batch)->i32=>0
fn make()->impl FnMut()->i32{var n=0;return move()=>{n+=1;return n}}
fn pack<F:FnMut()->i32>(callback:F)->State<F>{let b=batch();return State::<F>{r:R{code:90},items:give b.items,callback:callback,hits:0}}
 fn unused<F:FnMut()->i32>(state:State<F>)->State<F>{let a=give state.items;let old=state.items;return state} fn main()->i32=>0