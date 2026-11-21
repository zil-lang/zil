import std.iter
extern "C" fn putchar(value:i32)->i32;
resource Mark {code:i32}
impl Drop for Mark {fn drop(self:Self){unsafe{putchar(.code)}}}
package Item {manages mark:Mark; value:i32}
package Total {manages mark:Mark; value:i32}
fn main()->i32 {
    let values:Store<Item>=store_new()
    if !store_reserve(values,2){return 1}
    store_push(values,Item{mark:Mark{code:65},value:10})
    store_push(values,Item{mark:Mark{code:66},value:32})
    let cursor=iter.drain::<Item>(give values)
    var calls:i32=0
    var add=(previous:Total,item:Item)->Total=>{
        var next=previous
        next.value+=item.value
        calls+=1
        return next
    }
    let total=iter.fold_owned(cursor,Total{mark:Mark{code:90},value:0},&mut add)
    if calls!=2{return 2}
    return total.value-42
}
