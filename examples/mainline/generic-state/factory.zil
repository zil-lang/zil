import model
resource Ticket { code: i32 }
impl Drop for Ticket { fn drop(self: Self) { model.emit(.code) } }

pub fn make() -> impl FnMut(&mut model.Counter) -> i32 {
    let ticket = Ticket { code: 90 }
    let log = bytes_new()
    return move (value: &mut model.Counter) => {
        ticket.code
        if !bytes_reserve(log, bytes_len(log) + 1) { return 1 }
        if !bytes_push(log, 0u8) { return 1 }
        value.code += 1
        model.emit(value.code)
        return 0
    }
}
