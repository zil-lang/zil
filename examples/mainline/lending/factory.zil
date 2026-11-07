import helpers

resource Ticket { code: i32 }
impl Drop for Ticket { fn drop(self: Self) { helpers.emit(.code) } }

pub fn snapshot(value: u8) -> impl Fn() -> &[u8] {
    let ticket = Ticket { code: 90 }
    let bytes = bytes_new()
    bytes_reserve(bytes, 1)
    bytes_push(bytes, value)
    return move () => { ticket.code; return bytes_view(bytes) }
}

pub fn grow() -> impl FnMut(u8) -> &[u8] {
    let ticket = Ticket { code: 72 }
    let bytes = bytes_new()
    return move (value) => {
        ticket.code
        bytes_reserve(bytes, bytes_len(bytes) + 1)
        bytes_push(bytes, value)
        return bytes_view(bytes)
    }
}
