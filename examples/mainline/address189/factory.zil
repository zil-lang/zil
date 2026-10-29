import helpers

resource Ticket { code: i32 }
impl Drop for Ticket { fn drop(self: Self) { helpers.emit(.code) } }

pub fn make() -> impl FnMut(&mut i32) -> i32 {
    let ticket = Ticket { code: 72 }
    let bytes = bytes_new()
    return move (value: &mut i32) => {
        ticket.code
        if !bytes_reserve(bytes, bytes_len(bytes) + 1) { return -1 }
        if !bytes_push(bytes, u8(*value)) { return -2 }
        *value += 1
        return i32(bytes_len(bytes))
    }
}
