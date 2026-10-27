
extern "C" fn putchar(x:i32)->i32;
resource R{code:i32}impl Drop for R{fn drop(self:Self){unsafe{putchar(.code)}}}
package Holder<F>{prefix:[i64;2];callback:F;manages guard:R;last:u8}
fn pack<F:FnMut(i32)->i32>(callback:F)->Holder<F>{return Holder::<F>{prefix:[31i64,37i64],callback:callback,guard:R{code:84},last:9u8}}
fn make(seed:i32,mark:i32)->impl FnMut(i32)->i32{var n=seed;let r=R{code:mark};return move(delta:i32)=>{n+=delta;return n+r.code-mark}}
fn forward(seed:i32,mark:i32)->impl FnMut(i32)->i32{return make(seed,mark)}
fn relay<G:FnMut(i32)->i32>(s:Holder<G>)->Holder<G>{return s}
enum E{Fail}fn fail()->E!i32=>E.Fail;fn prepare(give r:R,x:i32)->impl FnMut(i32)->i32{return move(y:i32)=>r.code+x+y}fn main()->E!i32{var s=pack(make(1,65));s.callback=prepare(R{code:66},fail()?);return 0}