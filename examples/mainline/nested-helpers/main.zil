import factory
import helpers
import state
extern "C" fn getchar() -> i32;

fn main() -> i32 {
    var input = 0
    unsafe { input = getchar() }
    if input < 48 || input > 51 { return 9 }
    let snapshot = factory.snapshot(65u8)
    let first = helpers.peek(&snapshot)
    if first.len() != 1 { return 1 }
    helpers.emit(i32(first[0]))
    let current = state.make(factory.grow())
    let answer = state.run(current, i64(input - 48))
    if answer != 0 { return answer }
    let second = helpers.peek(&snapshot)
    helpers.emit(i32(second[0]))
    if first[0] != second[0] { return 8 }
    return 0
}
