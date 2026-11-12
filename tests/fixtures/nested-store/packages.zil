extern "C" fn putchar(n:i32)->i32;
resource Ticket{code:i32}
impl Drop for Ticket{fn drop(self:Self){unsafe{putchar(.code)}}}
package Item{manages ticket:Ticket;value:i32}
package Row{manages inner:Store<Item>;manages mark:Ticket}
fn main()->i32{
 let inner:Store<Item>=store_new()
 if !store_reserve(inner,1){return 1}
 store_push(inner,Item{ticket:Ticket{code:65},value:42})
 let outer:Store<Row>=store_new()
 if !store_reserve(outer,1){return 2}
 store_push(outer,Row{inner:give inner,mark:Ticket{code:82}})
 var total:i32=0
 if true {let row=store_at(outer,0);let item=store_at(row.inner,0);total=item.value}
 if true {let row=store_at_mut(outer,0);if !store_reserve(row.inner,2){return 3}}
 if !store_reserve(outer,2){return 4}
 match store_pop(outer){.ok(row)=>{let item=store_at(row.inner,0);if item.value!=42{return 5}},.err(_)=>{return 6}}
 return total-42
}
