extern "C" fn putchar(value: i32) -> i32;

pub enum Fault { Memory; Invalid }
pub resource Ticket { pub code: i32 }
impl Drop for Ticket { fn drop(self: Self) { unsafe { putchar(.code) } } }
pub package Item { manages ticket: Ticket; manages data: ByteBuffer; pub code: i32 }

pub fn make(code: i32) -> Fault!Item {
    let tag = Ticket { code: code }
    let data = bytes_new()
    if !bytes_reserve(data, 1) || !bytes_push(data, u8(code)) { return Fault.Memory }
    return Item { ticket: give tag, data: give data, code: code }
}

pub fn checked_code(item: Item) -> void!i32 {
    if bytes_len(item.data) != 1 { return () }
    let bytes = bytes_view(item.data)
    if bytes[0] != u8(item.code) { return () }
    return item.code
}

// This factory preserves an empty ByteBuffer when its own explicit reserve
// fails. Its complete result still has exactly one Ticket and buffer owner.
pub fn lossy(code: i32) -> Item {
    let tag = Ticket { code: code }
    let data = bytes_new()
    bytes_reserve(data, 1)
    bytes_push(data, u8(code))
    return Item { ticket: give tag, data: give data, code: code }
}
