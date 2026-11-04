extern "C" fn putchar(code:i32)->i32;
resource Token { code:i32 }
impl Drop for Token { fn drop(self:Self) { unsafe { putchar(.code) } } }
package Payload { manages token:Token; n:i32 }
enum Message { Data((Payload,Payload,Payload)), None }
fn result()->i32 {
    var value=Message.Data((Payload{token:Token{code:65},n:20},Payload{token:Token{code:66},n:22},Payload{token:Token{code:67},n:0}));
    match value {
        Message.Data((ref retained,moved,_))=>{let n=retained.n+moved.n;return n},
        Message.None()=>{return 0}
    }
}
