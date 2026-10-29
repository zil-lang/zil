
resource R{code:i32}
package State<F>{callback:F;manages owner:R}
fn pack<F:FnOnce()->i32>(f:F)->State<F>{return State::<F>{callback:f,owner:R{code:1}}}
fn once()->impl FnOnce()->i32{let r=R{code:7};return move()=>{let moved=give r;return moved.code}}
fn pure()->impl Fn()->i32{return ()=>7}
fn main()->i32{let s=pack(once());let f=s.callback;return f()-7}