import io
import events

resource Token { code: i32 }
impl Drop for Token { fn drop(self: Self) { io.emit(.code) } }
package Group {
    manages first: Token
    manages second: Token
    manages third: Token
    count: i32 = 3
}
fn relay(group: Group) -> Group { return group }
fn finish(group: Group, extra: i32) -> i32 {
    io.emit(88)
    return group.count + extra - 3
}
fn prepare(stage: i32) -> events.Fault!i32 {
    let outer = Token { code: 90 }
    let group = Group {
        third: Token { code: 67 + events.prepare(stage, 0)? },
        first: Token { code: 65 + events.prepare(stage, 1)? },
        second: Token { code: 66 + events.prepare(stage, 2)? },
        count: 3 + events.prepare(stage, 3)?
    }
    let moved = relay(group)
    return finish(moved, events.prepare(stage, 4)?)
}
fn loops() -> i32 {
    for i in 0..3 {
        let group = Group {
            third: Token { code: 67 },
            first: Token { code: 65 },
            second: Token { code: 66 }
        }
        if i == 0 { continue }
        if i == 1 { break }
    }
    return 0
}
fn main() -> events.Fault!i32 {
    let command = io.read()
    if command == 108 { return loops() }
    return prepare(command - 48)
}
