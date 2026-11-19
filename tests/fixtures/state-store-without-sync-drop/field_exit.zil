import state
import callbacks
extern "C" fn getchar() -> i32;

fn main() -> i32 {
    var input = 0
    unsafe { input = getchar() }
    if input < 48 || input > 51 { return 9 }
    let current = state.create(callbacks.make(72), input - 48)
    if !current.ready { return 1 }
    return state.take_then_exit(current)
}
