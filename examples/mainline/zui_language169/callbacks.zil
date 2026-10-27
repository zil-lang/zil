import model

pub fn make(code: i32) -> impl FnMut() -> i32 {
    let tag = model.ticket(code)
    var count = code
    return move () -> i32 => { count += 1; return count + tag.code - code }
}

pub fn forward(code: i32) -> impl FnMut() -> i32 { return make(code) }

pub fn wrap(code: i32) -> impl FnMut() -> i32 {
    var inner = forward(code)
    return move () -> i32 => inner()
}

pub fn run() -> i32 {
    var handlers = [wrap(65), wrap(66)]
    var old = handlers[0]
    handlers[0] = wrap(67)
    if old() != 66 || old() != 67 { return 1 }
    let first = &mut handlers[0]
    if first() != 68 { return 2 }
    let second = &mut handlers[1]
    if second() != 67 { return 3 }
    return 0
}
