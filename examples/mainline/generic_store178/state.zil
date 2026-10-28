import model
import collection

pub package State<F> { manages items: Store<model.Item>; callback: F }

pub fn make<F: FnMut() -> i32>(callback: F) -> State<F> {
    return State::<F> { items: collection.empty(), callback: callback }
}

pub fn run<F: FnMut() -> i32>(state: State<F>, count: i64) -> void!i32 {
    var current = state
    if !collection.reserve(current.items, count) { return () }
    var index: i64 = 0
    while index < count {
        collection.push(current.items, model.item(i32(index) + 65))
        index += 1
    }
    let callback = &mut current.callback
    let first = callback()
    let second = callback()
    if first != 1 || second != 2 { return 8 }
    return i32(collection.len(current.items)) - i32(count)
}
