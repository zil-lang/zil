extern "C" fn putchar(value: i32) -> i32;

pub enum Fault { Memory; Prepare }
pub resource Ticket { pub code: i32 }
impl Drop for Ticket { fn drop(self: Self) { unsafe { putchar(.code) } } }
pub package Item { manages ticket: Ticket; manages data: ByteBuffer; pub code: i32 }

pub resource Controller { pub count: i32 }
pub trait Read { fn read(self: &Self) -> i32; }
impl Read for Controller { fn read() -> i32 => .count }

pub fn item(code: i32) -> Item {
    return Item { ticket: Ticket { code: code }, data: bytes_new(), code: code }
}
pub fn prepare(code: i32, fail: bool) -> Fault!Item {
    let tag = Ticket { code: code }
    let data = bytes_new()
    if !bytes_reserve(data, 1) || !bytes_push(data, u8(code)) { return Fault.Memory }
    if fail { return Fault.Prepare }
    return Item { ticket: give tag, data: give data, code: code }
}
