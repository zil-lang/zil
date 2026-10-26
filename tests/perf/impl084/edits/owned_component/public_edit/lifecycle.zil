import io
import events
resource Token { code: i32 }
impl Drop for Token { fn drop(self: Self) { io.emit(.code) } }
package Group { manages first: Token; manages second: Token; manages third: Token; count: i32 = 0 }
fn make() -> Group { return Group { first: Token { code: 65 }, second: Token { code: 66 }, third: Token { code: 67 } } }
fn reader(group: Group) -> impl Fn() -> i32 { return move () => group.first.code + group.count }
fn updater() -> impl FnMut(i32) -> events.Fault!i32 {
    var group = make()
    return move (stage) => {
        if stage == 0 { group.first = Token { code: 68 + events.prepare(stage, 0)? } }
        if stage == 3 { group.first = Token { code: 68 } }
        if stage == 4 { let whole = group; group = whole }
        if stage == 7 {
            for i in 0..3 { let old = give group.first; group.first = Token { code: old.code + 1 } }
        }
        return group.first.code
    }
}
fn partial() -> impl FnOnce() -> i32 {
    let before = Token { code: 88 }
    let group = make()
    let after = Token { code: 89 }
    return move () => {
        let n = before.code - 88
        let removed = give group.second
        return n + removed.code - 66 + after.code - 89
    }
}
fn whole() -> impl FnOnce() -> i32 {
    let before = Token { code: 88 }
    let group = make()
    let after = Token { code: 89 }
    return move () => {
        let n = before.code - 88
        let moved = group
        return n + moved.count + after.code - 89
    }
}
fn fallible() -> impl FnOnce() -> events.Fault!i32 {
    let group = make()
    return move () => {
        let removed = give group.second
        return removed.code + events.prepare(0, 0)?
    }
}
fn main() -> events.Fault!i32 {
    let code = io.read()
    let outer = Token { code: 90 }
    if code == 114 {
        let f = reader(make())
        io.number(f(), 0); io.number(f(), 0)
        let moved = f
        return moved() - 65
    }
    if code == 117 { let unused = reader(make()); return 0 }
    if code == 48 { let f = partial(); return 0 }
    if code == 49 { let f = partial(); io.number(f(), 0); return 0 }
    if code == 50 { let f = whole(); io.number(f(), 0); return 0 }
    if code == 101 { let f = fallible(); return f()? }
    var update = updater()
    if code == 102 {
        let failed = update(0)
        io.emit(88)
        io.number(update(3)?, 0)
        return 0
    }
    if code == 52 { io.number(update(4)?, 0); io.number(update(4)?, 0); return 0 }
    if code == 55 { io.number(update(7)?, 0); io.number(update(7)?, 0); return 0 }
    return update(0)?
}
