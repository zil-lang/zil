import io
import events
resource Token { code: i32 }
impl Drop for Token { fn drop(self: Self) { io.emit(.code) } }
package Group { manages first: Token; manages second: Token; manages third: Token; count: i32 = 0 }
fn make() -> Group { return Group { first: Token { code: 65 }, second: Token { code: 66 }, third: Token { code: 67 } } }
fn relay(group: Group) -> Group { return group }
fn keep(give token: Token, n: i32) -> Token contract { result(give) } { return give token }
fn combine(give token: Token, give old: Token, n: i32) -> Token contract { result(give) } { return give token }
fn conditional(yes: bool) {
    var group = make()
    if yes { let old = give group.first }
    group.first = Token { code: 68 }
}
fn exercise(stage: i32) -> events.Fault!i32 {
    let outer = Token { code: 90 }
    var group = make()
    if stage == 0 { group.first = Token { code: 68 + events.prepare(stage, 0)? } }
    if stage == 1 { group.first = keep(Token { code: 68 }, events.prepare(stage, 1)?) }
    if stage == 2 { group.first = keep(give group.first, events.prepare(stage, 2)?) }
    if stage == 3 { group.first = Token { code: 68 } }
    if stage == 4 {
        let a = give group.first
        let b = give group.second
        group.first = give b
        group.second = give a
        let moved = relay(group)
        return moved.count
    }
    if stage == 5 {
        let a = give group.first
        let b = give group.second
        let c = give group.third
        group.count = 1
        return group.count - 1
    }
    if stage == 7 {
        for i in 0..3 {
            let old = give group.first
            group.first = Token { code: old.code + 1 }
        }
    }
    if stage == 8 { group.first = keep(give group.first, 0) }
    if stage == 9 { group.first = combine(Token { code: 68 }, give group.first, events.prepare(stage, 9)?) }
    return 0
}
fn main() -> events.Fault!i32 {
    let command = io.read()
    if command == 54 { conditional(true); conditional(false); return 0 }
    return exercise(command - 48)
}
