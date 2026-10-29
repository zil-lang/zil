
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
fn replace<C:FnMut()->i32>(input:Panel<C>)->Panel<C>{var next=input;let old=give next.tag;next.tag=Tag{code:89};let callback=&mut next.callback;let first=callback();next.hits=callback();return next}fn main()->i32{let current=replace(panel(handler()));return current.hits-2}