extern "C" fn putchar(value:i32)->i32;
pub resource Ticket { pub code:i32 }
impl Drop for Ticket { fn drop(self:Self){unsafe{putchar(.code)}} }
pub package P { manages ticket:Ticket; manages data:ByteBuffer; pub code:i32 }
pub fn make(code:i32)->P {
    let data=bytes_new()
    bytes_reserve(data,1)
    bytes_push(data,u8(code))
    return P{ticket:Ticket{code:code},data:give data,code:code}
}
pub fn failed()->P!i32 { return make(65) }
pub fn ready()->void!P { return make(66) }
pub fn consume(value:P)->i32 { return value.code }
