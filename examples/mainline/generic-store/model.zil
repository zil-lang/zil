extern "C" fn putchar(value: i32) -> i32;

pub resource Ticket { pub code: i32 }
impl Drop for Ticket { fn drop(self: Self) { unsafe { putchar(.code) } } }

pub package Item { manages ticket: Ticket; pub value: i32 }
pub package Other { manages ticket: Ticket; manages bytes: ByteBuffer; pub value: i64 }

pub fn item(code: i32) -> Item {
    return Item { ticket: Ticket { code: code }, value: code }
}

pub fn other(code: i32) -> void!Other {
    let bytes = bytes_new()
    if !bytes_reserve(bytes, 1) { return () }
    if !bytes_push(bytes, u8(code)) { return () }
    return Other { ticket: Ticket { code: code }, bytes: give bytes, value: i64(code) }
}

pub fn callback() -> impl FnMut() -> i32 {
    let ticket = Ticket { code: 72 }
    var count = 0
    return move () -> i32 => { count += 1; return count + ticket.code - 72 }
}
