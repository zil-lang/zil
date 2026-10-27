
extern "C" fn putchar(x:i32)->i32;
resource R{code:i32}impl Drop for R{fn drop(self:Self){unsafe{putchar(.code)}}}
package Holder<F>{prefix:[i64;2];callback:F;manages guard:R;last:u8}
fn pack<F:FnMut(i32)->i32>(callback:F)->Holder<F>{return Holder::<F>{prefix:[i64(31),i64(37)],callback:callback,guard:R{code:84},last:9u8}}
fn make(seed:i32,mark:i32)->impl FnMut(i32)->i32{var n=seed;let r=R{code:mark};return move(delta:i32)=>{n+=delta;return n+r.code-mark}}
fn forward(seed:i32,mark:i32)->impl FnMut(i32)->i32{return make(seed,mark)}
fn relay<G:FnMut(i32)->i32>(s:Holder<G>)->Holder<G>{return s}
fn main()->i32{var s=pack(make(1,65));s.callback=forward(2,66);return 0}