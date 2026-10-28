
resource Ticket{identity:i32} impl Drop for Ticket{fn drop(self:Self){}}
package Entry{manages ticket:Ticket;count:i32;coords:[i32;2]}
fn entry(n:i32)->Entry=>Entry{ticket:Ticket{identity:n},count:n,coords:[2,3]}
fn empty()->Store<Entry> contract{result(create)}{return store_new()}
fn inspect(p:&Entry)->i32=>p.count
fn escape(s:Store<Entry>)->&mut Entry{return store_at_mut(s,0)}fn main()->i32=>0;