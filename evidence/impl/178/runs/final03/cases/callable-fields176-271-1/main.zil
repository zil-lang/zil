
extern "C" fn putchar(x:i32)->i32;
resource Ticket{code:i32}
impl Drop for Ticket{fn drop(self:Self){unsafe{putchar(.code)}}}
package State<F>{manages ticket:Ticket;callback:F;hits:i32}
fn pack<F:FnMut()->i32>(callback:F, code:i32)->State<F>{return State::<F>{ticket:Ticket{code:code},callback:callback,hits:0}}
fn tick<G:FnMut()->i32>(state:State<G>)->State<G>{var next=state;let cb=&mut next.callback;next.hits=cb();return next}
fn make(n:i32)->impl FnMut()->i32{var value=n;return move ()=>{value+=1;return value}}
fn bytes()->impl FnMut()->i32{let buffer=bytes_new();bytes_reserve(buffer,4);bytes_push(buffer,7u8);return move()=>{let n=bytes_len(buffer);bytes_push(buffer,9u8);return i32(n)}}fn main()->i32{var state=pack(bytes(),84);state=tick(state);if state.hits!=1{return 1};state=tick(state);return state.hits-2}