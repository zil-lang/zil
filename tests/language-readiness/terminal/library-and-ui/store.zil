import std.store
extern "C" fn putchar(value:i32)->i32;
resource Token { value:i32 }
impl Drop for Token {fn drop(self:Self){unsafe{putchar(.value)}}}
package Item {manages token:Token;pub id:i32}
fn item(id:i32)->Item{return Item{token:Token{value:65+id},id:id}}
fn main()->void!i32 {
    let items:Store<Item>=store.new()
    if !store.reserve(items,4){return 91}
    store.push(items,item(0))
    store.push(items,item(1))
    store.push(items,item(2))
    let removed=store.remove(items,1)?
    if removed.id!=1 || store.len(items)!=2 || store_at(items,0).id!=0 || store_at(items,1).id!=2 {return 92}
    if store.capacity(items)!=4 {return 93}
    let final=store.pop(items)?
    if final.id!=2 || store.len(items)!=1 {return 94}
    let count=store.clear(items)
    if count!=1 || store.len(items)!=0 {return 95}
    return 0
}
