
extern "C" fn putchar(x:i32)->i32;
resource R{code:i32}impl Drop for R{fn drop(self:Self){unsafe{putchar(.code)}}}
package State<F>{callback:F;manages parent:R}
fn pack<F:Fn(i32)->i32>(f:F)->State<F>{return State::<F>{callback:f,parent:R{code:84}}}
fn make(give r:R,x:i32)->impl Fn(i32)->i32{return move(y:i32)=>r.code+x+y}
enum E{Fail}fn fail()->E!i32=>E.Fail;
fn main()->E!i32{var s=pack(make(R{code:65},1));s.callback=make(R{code:66},fail()?);return 0}
