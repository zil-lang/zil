import std.iter
extern "C" fn putchar(value:i32)->i32;
resource Mark {code:i32}
impl Drop for Mark {fn drop(self:Self){unsafe{putchar(.code)}}}
package Item {manages mark:Mark; value:i32}
fn run()->void!i32 {
    let values:Store<Item>=store_new()
    if !store_reserve(values,2){return ()}
    store_push(values,Item{mark:Mark{code:65},value:10})
    store_push(values,Item{mark:Mark{code:66},value:32})
    let nested:Store<iter.Drain<Item>>=store_new()
    if !store_reserve(nested,1){return ()}
    store_push(nested,iter.drain(give values))
    var cursor=store_pop(nested)?
    var total:i32=0
    var visit=(item:Item)=>{total+=item.value}
    iter.for_each_owned(&mut cursor,&mut visit)
    return total
}
fn main()->i32 {
    match run(){.err(_)=>{return 1},.ok(total)=>{return total-42}}
}
