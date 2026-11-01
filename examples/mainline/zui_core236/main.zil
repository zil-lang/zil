import portable
import model
import collections
import session

fn main() -> void!i32 {
    let count_input = portable.read()
    let mode_input = portable.read()
    if count_input < 48 || count_input > 51 || mode_input < 48 || mode_input > 52 { return 9 }
    let count = count_input - 48
    let mode = mode_input - 48
    var current = session.create(count)
    if !current.ready { return 1 }
    current = session.checked(current, count, 97)
    if !current.ready { return 8 }
    // No capture of Session: its new owning shape is not a capture permission.
    defer { portable.emit(81) }
    let late = model.Mark { code: 76 }
    portable.emit(83)
    // This branch leaves its lexical scope by normal fallthrough.
    if mode == 0 { defer { portable.emit(78) } }
    if mode == 1 { portable.emit(69); return 0 }
    if mode == 2 { portable.fail()?; return 8 }
    if mode == 3 {
        current = session.grow(current, i64(count + 1))
        if !current.ready { return 1 }
        current = session.checked(current, count, 97)
        if !current.ready { return 8 }
        var round = 0
        while round < 3 {
            defer { portable.emit(120) }
            let iteration = model.Mark { code: 105 }
            round += 1
            if round < 3 { continue }
            break
        }
    }
    if mode == 4 {
        current = session.replace(current, count)
        if !current.ready { return 1 }
    }
    if mode != 0 { portable.emit(78) }
    return 0
}
