extern "C" fn putchar(value: i32) -> i32;

pub enum Fault { Memory; Invalid }
pub resource Ticket { pub code: i32 }
impl Drop for Ticket { fn drop(self: Self) { unsafe { putchar(.code) } } }
pub struct Metadata { pub code: i32; pub coordinates: [i32; 2] }
pub package Item { manages ticket: Ticket; manages data: ByteBuffer; pub metadata: Metadata }

pub fn make(code: i32) -> Fault!Item {
    let ticket = Ticket { code: code }
    let data = bytes_new()
    if !bytes_reserve(data, 1) || !bytes_push(data, u8(code)) { return Fault.Memory }
    return Item {
        ticket: give ticket,
        data: give data,
        metadata: Metadata { code: code, coordinates: [code, code + 1] }
    }
}

pub fn emit(code: i32) { unsafe { putchar(code) } }
