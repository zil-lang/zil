extern "C" fn putchar(n:i32)->i32;
resource Ticket{code:i32}
impl Drop for Ticket{fn drop(self:Self){unsafe{putchar(.code)}}}
package Item{manages ticket:Ticket;value:i32}
fn main()->i32{
 let inner:Store<Item>=store_new()
 if !store_reserve(inner,1){return 1}
 store_push(inner,Item{ticket:Ticket{code:65},value:42})
 let outer:Store<Managed<Store<Item>>>=store_new()
 if !store_reserve(outer,1){return 2}
 store_push(outer,give inner)
 if !store_reserve(outer,2){return 3}
 match store_pop(outer){.ok(moved)=>{let item=store_at(moved,0);return item.value-42},.err(_)=>{return 4}}
}
