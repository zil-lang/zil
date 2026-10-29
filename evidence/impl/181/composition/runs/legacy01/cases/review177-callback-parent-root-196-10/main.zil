
extern "C" fn putchar(n:i32)->i32;
resource Tag { code:i32 }
impl Drop for Tag { fn drop(self:Self) { unsafe { putchar(.code) } } }
package Cell { manages tag:Tag; value:i32 }
package Batch { manages list:Store<Cell>; count:i32 }
package Panel<C> { manages tag:Tag; manages list:Store<Cell>; callback:C; hits:i32 }
fn batch(code:i32)->Batch {
    let list:Store<Cell>=store_new();
    if store_reserve(list,1) { store_push(list,Cell{tag:Tag{code:code},value:code}) }
    let count=i32(store_len(list));
    return Batch{list:give list,count:count}
}
fn count(list:Store<Cell>)->i32 { return i32(store_len(list)) }
fn consume(batch:Batch)->i32 { return batch.count }
fn handler()->impl FnMut()->i32 {
    let mark=Tag{code:87}; var calls=0;
    return move()=>{calls+=1;return calls+mark.code-87}
}
fn panel<C:FnMut()->i32>(callback:C)->Panel<C> {
    let prepared=batch(65);
    return Panel::<C>{tag:Tag{code:88},list:give prepared.list,callback:callback,hits:0}
}
fn main()->i32{var current=panel(handler());let view=store_at(current.list,0);let callback=&mut current.callback;let called=callback();return view.value}