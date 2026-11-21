import std.iter
extern "C" fn putchar(n:i32)->i32;
resource Ticket{code:i32}
impl Drop for Ticket{fn drop(self:Self){unsafe{putchar(.code)}}}
package Item{manages ticket:Ticket;value:i32}
fn main()->i32{
 let first:Store<Item>=store_new()
 let second:Store<Item>=store_new()
 if !store_reserve(first,1) || !store_reserve(second,1){return 1}
 store_push(first,Item{ticket:Ticket{code:65},value:20})
 store_push(second,Item{ticket:Ticket{code:66},value:22})
 let outer:Store<iter.Drain<Item>>=store_new()
 if !store_reserve(outer,2){return 2}
 store_push(outer,iter.drain(give first))
 store_push(outer,iter.drain(give second))
 var total:i32=0
 if true {let current=store_at_mut(outer,0);match current.next(){.ok(item)=>{total+=item.value},.err(_)=>{return 3}}}
 if !store_reserve(outer,4){return 4}
 match store_pop(outer){.ok(current)=>{var moved=current;match moved.next(){.ok(item)=>{total+=item.value},.err(_)=>{return 5}}},.err(_)=>{return 6}}
 return total-42
}
