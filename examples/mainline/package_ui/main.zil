import io
import events
import window

fn main() -> events.Fault!i32 {
    var app = window.relay(window.create())
    app = window.frame(app, 61)
    for i in 0..128 {
        let code = io.read()
        if code < 0 || code == 113 { return window.finish(app) }
        if code == 10 || code == 13 || code == 32 { continue }
        // The package argument is staged before decoding. Invalid input must
        // clean that prepared package exactly once when ? exits this scope.
        app = window.frame(app, events.decode(code)?)
    }
    return events.Fault.Limit
}
