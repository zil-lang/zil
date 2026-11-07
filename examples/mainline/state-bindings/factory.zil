import portable

resource Ticket { code: i32 }
impl Drop for Ticket { fn drop(self: Self) { portable.emit(.code) } }

pub fn active(label: i32) -> impl FnMut(&mut [u8]) -> i32 {
    let ticket = Ticket { code: label + 32 }
    let log = bytes_new()
    return move (values: &mut [u8]) => {
        ticket.code
        if !bytes_reserve(log, bytes_len(log) + 1) { return -1 }
        if !bytes_push(log, values[0]) { return -2 }
        var at: i64 = 0
        while at < values.len() {
            values[at] += 1u8
            at += 1
        }
        portable.emit(ticket.code - 32)
        return i32(bytes_len(log))
    }
}

pub fn once(label: i32) -> impl Fn() -> i32 {
    let ticket = Ticket { code: label + 32 }
    return move () => {
        portable.emit(ticket.code - 32)
        return 1
    }
}
