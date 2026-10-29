
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

package Bytes { manages data:ByteBuffer }
package BytePanel<C> { manages tag:Tag; manages data:ByteBuffer; callback:C; hits:i32 }
fn bytes()->Bytes { let data=bytes_new();bytes_reserve(data,1);bytes_push(data,9u8);return Bytes{data:give data} }
fn build<C:FnMut()->i32>(callback:C)->BytePanel<C> { let b=bytes();return BytePanel::<C>{tag:Tag{code:88},data:give b.data,callback:callback,hits:0} }
fn replace<C:FnMut()->i32>(input:BytePanel<C>)->BytePanel<C> { var next=input;let old=give next.data;let b=bytes();next.data=give b.data;let callback=&mut next.callback;next.hits=callback();return next }
fn main()->i32 { var current=replace(build(handler()));let callback=&mut current.callback;if callback()!=2{return 1}return i32(bytes_len(current.data))-1 }
