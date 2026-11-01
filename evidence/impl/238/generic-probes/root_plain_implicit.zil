extern "C" fn putchar(code:i32)->i32;
resource Ticket{code:i32}
impl Drop for Ticket{fn drop(self:Self){unsafe{putchar(.code)}}}
package Item{manages ticket:Ticket;value:i32}
package Root<T:StoreElement>{manages items:Store<T>}
fn relay<T:StoreElement>(root:Root<T>)->Root<T>{return root}
fn main()->i32{
 let items:Store<Item>=store_new()
 if !store_reserve(items,1){return 1}
 store_push(items,Item{ticket:Ticket{code:65},value:3})
 var current=Root::<Item>{items:give items}
 defer{unsafe{putchar(81)}}
 let late=Ticket{code:76}
 current=relay(current)
 return 0
}
