import model
import collection
pub package State<F> { manages items: Store<model.Item>; callback: F; total: i32 }
pub fn make<F: FnMut() -> i32>(callback: F) -> State<F> {
    return State::<F> { items: collection.empty(), callback: callback, total: 0 }
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
    index = 0
    while index < count {
        let item = store_at_mut(current.items, index)
        let increment = callback()
        item.value += increment
        current.total += increment
        collection.reserve(current.items, count + 20)
        let again = callback()
        if item.value != 66 + 3 * i32(index) || again != 2 * i32(index) + 2 { return 10 }
        index += 1
    }
    if current.total != i32(count * count) { return 11 }
    let old_capacity = store_capacity(current.items)
    let grown = collection.reserve(current.items, count + 2)
    if !grown && store_capacity(current.items) != old_capacity { return 12 }
    if collection.len(current.items) != count { return 13 }
    index = 0
    while index < count {
        let item = store_at(current.items, index)
        if item.value != 66 + 3 * i32(index) { return 14 }
        index += 1
    }
    return 0
}
