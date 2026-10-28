import model

pub fn fill(items: Store<model.Item>, count: i32, start: i32) -> bool {
    if !store_reserve(items, i64(count)) { return false }
    var index = 0
    while index < count {
        store_push(items, model.item(start + index))
        index += 1
    }
    return store_len(items) == i64(count)
}

pub fn read(item: &model.Item) -> i32 { return item.code }
pub fn sum(items: Store<model.Item>) -> i32 {
    var index: i64 = 0
    var total = 0
    while index < store_len(items) {
        let item = store_at(items, index)
        total += read(item)
        index += 1
    }
    return total
}

// Concrete helpers keep the current parametric checker contract explicit.
pub fn empty() -> Store<model.Item> contract { result(create) } { return store_new() }

pub fn grow(items: Store<model.Item>, capacity: i64) -> bool {
    let old_len = store_len(items)
    let old_capacity = store_capacity(items)
    let old_sum = sum(items)
    let grown = store_reserve(items, capacity)
    if store_len(items) != old_len { return false }
    if !grown && store_capacity(items) != old_capacity { return false }
    return sum(items) == old_sum
}
