
extern "C" fn putchar(x:i32)->i32;
resource R{code:i32}impl Drop for R{fn drop(self:Self){unsafe{putchar(.code)}}}
package State<F>{manages ticket:R;callback:F}
fn pack<F:FnOnce()->i32>(callback:F)->State<F>{return State::<F>{ticket:R{code:84},callback:callback}}
fn make(n:i32)->impl FnOnce()->i32{let r=R{code:n};return move()=>{let value=give r;return value.code}}
fn main()->i32{let state=pack(make(65));let once=state.callback;return once()-65}
