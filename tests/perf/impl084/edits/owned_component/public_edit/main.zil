import io
import events
import window
import helpers
fn main() -> events.Fault!i32 {
    var update = window.handler()
    io.number(helpers.apply(&mut update, 61)?, 0)
    io.emit(10)
    for i in 0..128 {
        let code = io.read()
        if code < 0 || code == 113 {
            io.emit(35)
            io.number(helpers.apply(&mut update, 35)?, 0)
            io.emit(10)
            return 0
        }
        if code == 10 || code == 13 || code == 32 { continue }
        io.number(helpers.apply(&mut update, code)?, 0)
        io.emit(10)
    }
    return events.Fault.Limit
}
