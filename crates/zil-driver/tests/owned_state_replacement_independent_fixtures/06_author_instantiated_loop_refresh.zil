extern "C" fn putchar(n:i32)->i32;
resource Ticket{code:i32}
impl Drop for Ticket{fn drop(self:Self){unsafe{putchar(.code)}}}
fn mark(){unsafe{putchar(88)}}
package State<T,F>{manages bytes:ByteBuffer;a:F;b:F;tag:T}
fn recycle<T:Copy,F:Fn()->i32>(state:State<T,F>)->State<T,F>{var next=state;var view=&next.a;while view()==0 {let moved=next.a;next.a=moved;view=&next.a}view();return next}
fn make(label:i32)->impl Fn()->i32{let ticket=Ticket{code:label};return move()=>{ticket.code;return 1}}
fn pack<T:Copy,F:Fn()->i32>(a:F,b:F,tag:T)->State<T,F>{return State::<T,F>{bytes:bytes_new(),a:a,b:b,tag:tag}}
fn main()->i32{let state=pack(make(65),make(66),7);let next=recycle(state);return next.tag-7}
