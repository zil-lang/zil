import factory
import helpers
import state
extern "C" fn getchar() -> i32;

fn main() -> i32 {
    var input = 0
    unsafe { input = getchar() }
    if input < 48 || input > 51 { return 9 }
    var value = 65
    var callback = factory.make()
    let first = helpers.append(&mut callback, &mut value)
    if first != 1 { return 8 }
    helpers.emit(value - 1)
    let current = state.make(callback, value)
    return state.run(current, i64(input - 48))
}
