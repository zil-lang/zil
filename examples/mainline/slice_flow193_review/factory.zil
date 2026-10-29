import helpers
resource Ticket { code: i32 }
impl Drop for Ticket { fn drop(self: Self) { helpers.emit(.code) } }
pub fn make() -> impl FnMut(&mut [u8], &mut i32) -> i32 {
    let ticket = Ticket { code: 72 }
    let history = bytes_new()
    return move (window: &mut [u8], counter: &mut i32) => {
        ticket.code
        if !bytes_reserve(history, bytes_len(history) + 1) { return -1 }
        if !bytes_push(history, u8(*counter)) { return -2 }
        window[0] = u8(*counter)
        *counter += 1
        return i32(bytes_len(history))
    }
}
