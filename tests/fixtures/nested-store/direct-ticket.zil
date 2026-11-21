extern "C" fn putchar(n:i32)->i32;
resource Ticket{code:i32}
impl Drop for Ticket{fn drop(self:Self){unsafe{putchar(.code)}}}
fn main()->i32{
 let outer:Store<Managed<Ticket>>=store_new()
 if !store_reserve(outer,2){return 1}
 let first=Ticket{code:65}
 store_push(outer,give first)
 store_push(outer,Ticket{code:66})
 if !store_reserve(outer,4){return 2}
 match store_pop(outer){.ok(moved)=>{let item=moved},.err(_)=>{return 3}}
 return 0
}
