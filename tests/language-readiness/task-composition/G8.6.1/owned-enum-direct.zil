extern "C" fn putchar(code:i32)->i32;
resource Token { code:i32 }
impl Drop for Token { fn drop(self:Self) { unsafe { putchar(.code); } } }
enum Packet { Empty, Item(manages Token) }
fn worker(give token:Token)->i32 {
    let packet=Packet.Item(give token);
    match packet {
        Packet.Empty()=>{return 1;},
        Packet.Item(value)=>{return value.code;}
    }
}
fn main()->i32 { let input=Token{code:65}; let value=worker(give input); return 0; }
