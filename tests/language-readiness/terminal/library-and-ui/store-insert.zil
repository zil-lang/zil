import std.store
extern "C" fn putchar(value:i32)->i32;
resource Token { value:i32 }
impl Drop for Token {fn drop(self:Self){unsafe{putchar(.value)}}}
package Item {manages token:Token;pub id:i32}
fn item(id:i32)->Item{return Item{token:Token{value:65+id},id:id}}
fn main()->Item!i32 {
    let items:Store<Item>=store.new()
    if !store.reserve(items,6){return 91}
    store.insert(items,0,item(0))?;store.push(items,item(1));store.push(items,item(2))
    store.insert(items,1,item(3))?
    store.insert(items,0,item(4))?
    store.insert(items,5,item(5))?
    if store.len(items)!=6 || store.capacity(items)!=6 {return 92}
    if store_at(items,0).id!=4 || store_at(items,1).id!=0 || store_at(items,2).id!=3 || store_at(items,3).id!=1 || store_at(items,4).id!=2 || store_at(items,5).id!=5 {return 93}
    if store.truncate(items,-1) || store.truncate(items,7) || store.len(items)!=6 {return 94}
    if !store.truncate(items,3) || store.len(items)!=3 {return 95}
    return 0
}
