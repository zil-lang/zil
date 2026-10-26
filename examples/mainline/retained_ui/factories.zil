import io
import events

resource Token { code: i32; count: i32 }
impl Drop for Token { fn drop(self: Self) { io.emit(.code) } }
fn consume(give token: Token) -> i32 { return token.count }

pub fn reader(seed: i32) -> impl Fn() -> i32 {
    let token = Token { code: 66, count: seed }
    return move () => token.count
}

pub fn counter(seed: i32) -> impl FnMut(i32) -> i32 {
    // F is destroyed when the factory returns; A remains in the returned value.
    let temporary = Token { code: 70, count: 0 }
    let token = Token { code: 65, count: seed }
    return move (delta) => { token.count += delta; return token.count }
}

pub fn once(seed: i32) -> impl FnOnce() -> i32 {
    let first = Token { code: 67, count: seed }
    let second = Token { code: 68, count: 0 }
    return move () => { let result = consume(give first); return result + second.count }
}

pub fn fallible(stop: i32) -> impl FnMut() -> events.Fault!i32 {
    let token = Token { code: 69, count: 0 }
    return move () => {
        token.count += 1
        io.number(token.count)
        if token.count == stop { return events.Fault.Stop(stop) }
        return token.count
    }
}

pub fn snapshot(seed: i32) -> impl Fn(i32) -> i32 {
    return move (value) => seed + value
}
