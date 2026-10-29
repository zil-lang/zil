import model

pub package Batch { manages items: Store<model.Item> }

pub fn prepare(count: i32, first: i32) -> void!Batch {
    let items: Store<model.Item> = store_new()
    if count > 0 && !store_reserve(items, i64(count)) { return () }
    for index in 0..count {
        store_push(items, model.make(first + index)?)
    }
    if store_len(items) != i64(count) { return () }
    return Batch { items: give items }
}

pub fn into_store(batch: Batch) -> Store<model.Item> contract { result(give) } {
    return give batch.items
}

pub fn check_tail(items: Store<model.Item>, expected: i32) -> void!i32 {
    let tail = model.checked(store_pop(items)?)?
    let code = tail.code
    store_push(items, tail)
    if code != expected { return () }
    return 0
}
