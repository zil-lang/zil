extern "C" fn putchar(code:i32)->i32;
resource Token { code:i32 }
impl Drop for Token { fn drop(self:Self) { unsafe { putchar(.code) } } }
package Payload { manages token:Token; n:i32 }
fn identity(pair:(Payload,i32))->(Payload,i32) { return pair }
fn result()->i32 {
    let (payload,n)=identity((Payload{token:Token{code:65},n:20},22));
    return payload.n+n
}
