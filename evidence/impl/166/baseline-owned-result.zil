extern "C" fn putchar(value:i32)->i32;
resource R { code:i32 }
impl Drop for R { fn drop(self:Self) { unsafe { putchar(.code) } } }
package P { manages r:R }
enum E { Fail }
fn make()->E!P=>P { r:R { code:65 } }
fn main()->E!i32 { let p=make()?; return p.r.code-65 }
