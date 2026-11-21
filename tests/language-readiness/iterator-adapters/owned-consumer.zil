import std.iter
extern "C" fn putchar(value:i32)->i32;
resource Mark {code:i32}
impl Drop for Mark {fn drop(self:Self){unsafe{putchar(.code)}}}
package Item {manages mark:Mark; value:i32}
fn main()->i32 {
    let values:Store<Item>=store_new()
    if !store_reserve(values,2){return 1}
    store_push(values,Item{mark:Mark{code:65},value:10})
    store_push(values,Item{mark:Mark{code:66},value:32})
    var cursor=iter.drain::<Item>(give values)
    var total:i32=0
    var visit=(item:Item)=>{total+=item.value}
    iter.for_each_owned(&mut cursor,&mut visit)
    return total-42
}
