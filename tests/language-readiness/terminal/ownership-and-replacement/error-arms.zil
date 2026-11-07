extern "C" fn putchar(code:i32)->i32;
resource Token { code:i32 }
impl Drop for Token { fn drop(self:Self) { unsafe { putchar(.code) } } }
package Good { manages token:Token; n:i32 }
package Failure { manages token:Token; n:i32 }
fn make(ok:bool)->Failure!Good {
    if ok { return Good { token:Token { code:65 }, n:20 } }
    return Failure { token:Token { code:66 }, n:22 }
}
fn work(ok:bool)->i32 {
    var n=0;
    match make(ok) {
        .ok(p)=>{ let inside=p.n; n=inside },
        .err(p)=>{ let inside=p.n; n=inside }
    }
    unsafe { putchar(88) }
    return n;
}
fn result()->i32 { return work(true)+work(false) }
