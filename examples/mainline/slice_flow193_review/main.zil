import factory
import state
extern "C" fn getchar() -> i32;
fn main() -> i32 {
    var input = 0
    unsafe { input = getchar() }
    if input < 48 || input > 55 { return 9 }
    return state.run(state.make(factory.make()), i64(input - 48))
}
