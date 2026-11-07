import state
extern "C" fn getchar() -> i32;

fn main() -> void!i32 {
    var count: i32 = 0
    unsafe { count = getchar() - 48 }
    if count < 0 || count > 3 { return 9 }
    let initial = state.make()?
    let updated = state.replace(initial, count)?
    let moved = state.relay(updated)
    return moved.revision - 1
}
