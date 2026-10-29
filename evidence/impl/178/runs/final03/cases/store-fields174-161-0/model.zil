extern "C" fn putchar(value: i32) -> i32;

pub resource Ticket { pub code: i32 }
impl Drop for Ticket { fn drop(self: Self) { unsafe { putchar(.code) } } }
pub package Item { manages ticket: Ticket; manages data: ByteBuffer; pub code: i32 }

pub fn make(code: i32) -> void!Item {
    let ticket = Ticket { code: code }
    let data = bytes_new()
    if !bytes_reserve(data, 1) || !bytes_push(data, u8(code)) { return () }
    return Item { ticket: give ticket, data: give data, code: code }
}

pub fn checked(item: Item) -> void!Item {
    if bytes_len(item.data) != 1 { return () }
    let bytes = bytes_view(item.data)
    if bytes[0] != u8(item.code) { return () }
    return item
}
