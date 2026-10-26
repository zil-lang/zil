import io
import events
import helpers

resource Token { code: i32; count: i32 }
impl Drop for Token { fn drop(self: Self) { io.emit(.code) } }
fn consume(give item: Token) -> i32 { return item.count }
fn accept<F: FnOnce() -> i32>(f: F, give item: Token, code: i32) -> i32 {
    io.emit(72)
    return f() + item.count + code
}
fn check(stop: bool) -> events.Fault!i32 {
    if stop { return events.Fault.Prepare(1) }
    return 0
}
fn shared() -> i32 {
    let a = Token { code: 65, count: 1 }
    let f = move () => a.count
    io.number(helpers.sample_twice(&f), 0)
    io.number(f(), 0)
    return 0
}
fn exclusive() -> i32 {
    let a = Token { code: 65, count: 0 }
    var f = move () => { a.count = a.count + 1; return a.count }
    io.number(helpers.update_twice(&mut f), 0)
    io.number(f(), 0)
    return 0
}
fn conditional(enabled: bool) -> i32 {
    let a = Token { code: 65, count: 1 }
    let b = Token { code: 66, count: 0 }
    let f = move () => { let result = consume(give a); return result + b.count }
    io.number(helpers.run_if(f, enabled), 0)
    return 0
}
fn adaptation(exclusive: bool) -> i32 {
    let a = Token { code: 65, count: 3 }
    if exclusive {
        var f = move () => { a.count = a.count + 1; return a.count }
        io.number(helpers.run_if(f, true), 0)
        return 0
    }
    let f = move () => a.count
    io.number(helpers.run_if(f, true), 0)
    return 0
}
fn shared_as_mut() -> i32 {
    let a = Token { code: 65, count: 3 }
    var f = move () => a.count
    io.number(helpers.update_twice(&mut f), 0)
    io.number(f(), 0)
    return 0
}
fn copy_as_once() -> i32 {
    let n = 7
    let f = () => n
    io.number(helpers.run_if(f, true), 0)
    return 0
}
fn prepared(stop: bool) -> events.Fault!i32 {
    let outer = Token { code: 90, count: 0 }
    let a = Token { code: 65, count: 1 }
    let f = move () => consume(give a)
    let b = Token { code: 66, count: 2 }
    let result = accept(f, give b, check(stop)?)
    io.number(result, 0)
    return 0
}
fn errors(stop: i32) -> events.Fault!i32 {
    let a = Token { code: 65, count: 0 }
    var f = move () -> events.Fault!i32 => {
        a.count = a.count + 1
        io.number(a.count, 0)
        if a.count == stop { return events.Fault.Prepare(stop) }
        return a.count
    }
    let result = helpers.try_twice_mut(&mut f)?
    io.emit(58)
    io.number(result, 0)
    io.emit(58)
    io.number(f()?, 0)
    return 0
}
fn shared_errors(stop: i32) -> events.Fault!i32 {
    let a = Token { code: 65, count: 0 }
    let f = () -> events.Fault!i32 => {
        a.count = a.count + 1
        io.number(a.count, 0)
        if a.count == stop { return events.Fault.Prepare(stop) }
        return a.count
    }
    let result = helpers.try_twice(&f)?
    io.emit(58)
    io.number(result, 0)
    return 0
}
fn main() -> events.Fault!i32 {
    let code = io.read()
    if code == 115 { return shared() }
    if code == 109 { return exclusive() }
    if code == 48 { return conditional(false) }
    if code == 49 { return conditional(true) }
    if code == 97 { return adaptation(false) }
    if code == 98 { return adaptation(true) }
    if code == 111 { return shared_as_mut() }
    if code == 99 { return copy_as_once() }
    if code == 112 { return prepared(true) }
    if code == 80 { return prepared(false) }
    if code == 101 { return errors(1) }
    if code == 102 { return errors(2) }
    if code == 69 { return errors(0) }
    if code == 100 { return shared_errors(1) }
    if code == 68 { return shared_errors(0) }
    return events.Fault.Invalid(code)
}
