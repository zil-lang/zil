import std.iter
extern "C" fn putchar(value:i32)->i32;
resource Mark {code:i32}
impl Drop for Mark {fn drop(self:Self){unsafe{putchar(.code)}}}
package Item {manages mark:Mark; value:i32}
package Cursor {manages values:Store<Item>; index:i64}
impl iter.LendingIterator for Cursor {
    type Item=Item;
    fn next<life a>(self:&'a mut Self)->Option<&'a Item> {
        if self.index>=store_len(self.values){return Option::<&'a Item>.None()}
        let index=self.index
        self.index+=1
        return Option::<&'a Item>.Some(store_at(self.values,index))
    }
}
fn main()->i32 {
    let values:Store<Item>=store_new()
    if !store_reserve(values,2){return 1}
    store_push(values,Item{mark:Mark{code:65},value:10})
    store_push(values,Item{mark:Mark{code:66},value:32})
    var cursor=Cursor{values:give values,index:0}
    var total:i32=0
    var visit=(item:&Item)=>{total+=item.value}
    iter.for_each_lent(&mut cursor,&mut visit)
    return total-42
}
