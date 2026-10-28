
resource Ticket{identity:i32} impl Drop for Ticket{fn drop(self:Self){}}
package Entry{manages ticket:Ticket;count:i32;coords:[i32;2]}
fn entry(n:i32)->Entry=>Entry{ticket:Ticket{identity:n},count:n,coords:[2,3]}
fn empty()->Store<Entry> contract{result(create)}{return store_new()}
fn inspect(p:&Entry)->i32=>p.count
fn main()->i32{let s=empty();store_reserve(s,4);store_push(s,entry(10));store_push(s,entry(20));let held=store_at(s,1);var change=()=>{let p=store_at_mut(s,0);p.count=30;return p.count};return change()+held.count}