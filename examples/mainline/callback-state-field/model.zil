
extern "C" fn putchar(x:i32)->i32;
pub resource Ticket{code:i32}
impl Drop for Ticket{fn drop(self:Self){unsafe{putchar(.code)}}}
pub package State<F>{manages ticket:Ticket;pub callback:F;pub hits:i32}
pub fn pack<F:FnMut()->i32>(callback:F, code:i32)->State<F>{return State::<F>{ticket:Ticket{code:code},callback:callback,hits:0}}
pub fn tick<G:FnMut()->i32>(state:State<G>)->State<G>{var next=state;let cb=&mut next.callback;next.hits=cb();return next}
pub fn make(n:i32)->impl FnMut()->i32{var value=n;return move ()=>{value+=1;return value}}
 pub fn twice<H:FnMut()->i32>(state:State<H>)->State<H>{return tick(tick(state))}