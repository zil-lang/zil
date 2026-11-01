
extern "C" fn putchar(code:i32)->i32;
resource Ticket{code:i32}
impl Drop for Ticket{fn drop(self:Self){unsafe{putchar(.code)}}}
package Item{manages ticket:Ticket;value:i32}
package Bag{manages items:Store<Item>}
package Pair{left:Bag;right:Bag}
fn bag(code:i32)->Bag{
 let items:Store<Item>=store_new();
 if store_reserve(items,1){store_push(items,Item{ticket:Ticket{code:code},value:code})}
 return Bag{items:give items}
}
fn pair()->Pair{return Pair{left:bag(65),right:bag(66)}}
package GenericBag<T:StoreElement>{manages items:Store<T>}package GenericRoot<T:StoreElement>{child:GenericBag<T>}fn wrap<T:StoreElement>(give items:Store<T>)->GenericRoot<T>{return GenericRoot::<T>{child:GenericBag::<T>{items:give items}}}fn main()->i32{let items:Store<Item>=store_new();let root=wrap(give items);let child=root.child;return i32(store_len(child.items))}