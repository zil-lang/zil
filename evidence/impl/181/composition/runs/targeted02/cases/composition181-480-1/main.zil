import model
import collection
import state
extern "C" fn getchar() -> i32;
fn main() -> void!i32 {
    var input = 0
    unsafe { input = getchar() }
    if input < 48 || input > 51 { return 9 }
    let current = state.make(model.callback())
    let other: Store<model.Other> = collection.empty()
    if !collection.reserve(other, 1) { return () }
    collection.push(other, model.other(90)?)
    let moved = collection.relay(give other)
    let result = state.run(current, i64(input - 48))?
    let item = collection.pop(moved)?
    if item.value != 90 || collection.len(moved) != 0 { return 7 }
    return result
}
