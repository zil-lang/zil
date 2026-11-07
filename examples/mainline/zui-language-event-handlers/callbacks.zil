import model

pub fn make(code: i32) -> impl FnMut() -> i32 {
    let ticket = model.Ticket { code: code }
    var count = code
    return move () -> i32 => { count += 1; return count + ticket.code - code }
}
pub fn wrap(code: i32) -> impl FnMut() -> i32 {
    var inner = make(code)
    return move () -> i32 => inner()
}
