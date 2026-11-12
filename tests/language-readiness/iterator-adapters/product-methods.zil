extern "C" fn putchar(value:i32)->i32;
resource Mark {code:i32}
impl Drop for Mark {fn drop(self:Self){unsafe{putchar(.code)}}}
package Item {manages mark:Mark; value:i32}
trait Iterator {type Item:StoreElement;fn next(self:&mut Self)->void!Self.Item;}
package Drain<T:StoreElement>{manages items:Store<T>}
impl<T:StoreElement> Iterator for Drain<T>{
    type Item:StoreElement=T;
    fn next(self:&mut Self)->void!T{return store_pop(self.items)}
}
fn drain<T:StoreElement>(give items:Store<T>)->Drain<T>{return Drain::<T>{items:give items}}
fn advance<I:Iterator>(cursor:&mut I)->void!I.Item{return cursor.next()}
fn consume<T:StoreElement>(value:T){}
fn main()->i32{
    let values:Store<Item>=store_new()
    if !store_reserve(values,2){return 1}
    store_push(values,Item{mark:Mark{code:65},value:10})
    store_push(values,Item{mark:Mark{code:66},value:32})
    var cursor=drain::<Item>(give values)
    match advance(&mut cursor){
        .ok(value)=>{if value.value!=32{return 2};consume(value)},
        .err(exhausted)=>{return 3}
    }
    return 0
}
