extern "C" fn putchar(value:i32)->i32;
resource Ticket{code:i32}
impl Drop for Ticket{fn drop(self:Self){unsafe{putchar(.code)}}}
fn identity<T:Value>(value:T)->T{return value}
fn observe<T:Copy>(value:Ticket,count:T)->T{return count}
fn main()->i32{
 let owner=Ticket{code:65}
 let moved=identity::<Managed<Ticket>>(give owner)
 let forwarded=identity(moved)
 let fresh=identity(Ticket{code:66})
 return observe(Ticket{code:67},42)-42
}
