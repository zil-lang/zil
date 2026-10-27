// One language consumer: actual head borrow, persistent state fields,
// runtime-sized ownership, ordinary helper transfers, and retained callbacks.
import model
import panel
import collection
import callbacks
extern "C" fn getchar() -> i32;

fn main() -> void!i32 {
    var input = 0
    unsafe { input = getchar() }
    if input < 48 || input > 52 { return 9 }
    let controller = model.Controller { count: input - 48 }
    let count = controller.read()
    var state = panel.make()
    var children = state.children
    let old = children[0]
    children[0] = model.item(67)
    state.children = children
    let saved = panel.relay(state)
    let initial = collection.empty()
    let storage = collection.relay(give initial)
    if !store_reserve(storage, i64(count)) { return () }
    var i = 0
    while i < count {
        collection.insert(storage, model.item(68 + i))
        i += 1
    }
    if store_len(storage) != i64(count) { return 2 }
    var sum = 0
    while store_len(storage) > 0 {
        let last = collection.take_last(storage)?
        sum += last.code
    }
    if sum != count * 68 + count * (count - 1) / 2 { return 3 }
    if saved.revision != 1 || old.code != 65 { return 4 }
    var callback = callbacks.wrap(72)
    if callback() != 73 || callback() != 74 { return 5 }
    return 0
}
