import io
import events
import factories
import helpers

resource Guard { code: i32; value: i32 }
impl Drop for Guard { fn drop(self: Self) { io.emit(.code) } }
fn retain(give guard: Guard) -> impl Fn() -> i32 { return move () => guard.value }
fn accept<F: FnOnce() -> i32>(f: F, give guard: Guard, code: i32) -> i32 {
    io.emit(72)
    return f() + guard.value + code
}
fn prepared(stop: bool) -> events.Fault!i32 {
    let outer = Guard { code: 90, value: 0 }
    let f = factories.once(1)
    let item = Guard { code: 66, value: 2 }
    let value = accept(f, give item, events.check(stop)?)
    io.number(value)
    return 0
}
fn errors(stop: i32) -> events.Fault!i32 {
    var f = factories.fallible(stop)
    let value = helpers.try_twice(&mut f)?
    io.emit(58)
    io.number(value)
    io.emit(58)
    io.number(f()?)
    return 0
}
fn event_loop() -> events.Fault!i32 {
    // The factory has returned before any event arrives. The inline environment
    // is retained by this invocation across the complete synchronous event loop.
    var update = factories.counter(0)
    io.number(update(0))
    io.emit(10)
    for i in 0..32 {
        let code = io.read()
        if code < 0 || code == 113 { return 0 }
        if code == 10 || code == 32 { continue }
        io.number(update(events.delta(code)?))
        io.emit(10)
    }
    return events.Fault.Limit
}
fn main() -> events.Fault!i32 {
    let mode = io.read()
    if mode == 108 { return event_loop() }
    if mode == 115 {
        let f = factories.reader(21)
        io.number(helpers.sample_twice(&f))
        let moved = f
        io.number(moved())
        return 0
    }
    if mode == 122 {
        let unused = factories.reader(1)
        io.emit(90)
        return 0
    }
    if mode == 109 {
        var f = factories.counter(0)
        io.number(helpers.update_twice(&mut f))
        io.number(f(1))
        return 0
    }
    if mode == 48 || mode == 49 {
        let f = factories.once(1)
        io.number(helpers.run_if(f, mode == 49))
        return 0
    }
    if mode == 112 { return prepared(true) }
    if mode == 80 { return prepared(false) }
    if mode == 101 { return errors(1) }
    if mode == 102 { return errors(2) }
    if mode == 69 { return errors(0) }
    if mode == 99 {
        let f = factories.snapshot(20)
        io.number(f(1) + f(1))
        return 0
    }
    if mode == 103 {
        let guard = Guard { code: 71, value: 7 }
        let f = retain(give guard)
        io.number(f())
        io.number(f())
        return 0
    }
    return events.Fault.Invalid(mode)
}
