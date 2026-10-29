
extern "C" fn putchar(x:i32)->i32;
resource Ticket{code:i32}
impl Drop for Ticket{fn drop(self:Self){unsafe{putchar(.code)}}}
package State<F>{manages ticket:Ticket;callback:F;hits:i32}
fn pack<F:FnMut()->i32>(callback:F, code:i32)->State<F>{return State::<F>{ticket:Ticket{code:code},callback:callback,hits:0}}
fn tick<G:FnMut()->i32>(state:State<G>)->State<G>{var next=state;let cb=&mut next.callback;next.hits=cb();return next}
fn make(n:i32)->impl FnMut()->i32{var value=n;return move ()=>{value+=1;return value}}

fn owned(n:i32)->impl FnMut()->i32{let resource=Ticket{code:n};var value=n;return move()=>{value+=1;return value+resource.code-n}}
fn forward(n:i32)->impl FnMut()->i32{return owned(n)}
fn nested(n:i32)->impl FnMut()->i32{var child=owned(n);return move()=>child()}
 fn main()->i32{let f=make(1);let state=pack(f,84);let duplicate=pack(f,85);return 0}