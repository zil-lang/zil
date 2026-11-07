import model

pub fn make(code: i32) -> impl FnMut() -> i32 {
    let guard = model.Ticket { code: code }
    var calls = 0
    return move () -> i32 => {
        calls += 1
        return calls + guard.code - code
    }
}
