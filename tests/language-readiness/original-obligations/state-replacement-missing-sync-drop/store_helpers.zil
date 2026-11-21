pub fn reserve<T: StoreElement>(items: Store<T>, capacity: i64) -> bool {
    return store_reserve(items, capacity)
}
pub fn push<T: StoreElement>(items: Store<T>, item: T) -> T!void {
    return store_push(items, item)
}
pub fn pop<T: StoreElement>(items: Store<T>) -> void!T { return store_pop(items) }
pub fn len<T: StoreElement>(items: Store<T>) -> i64 { return store_len(items) }
pub fn capacity<T: StoreElement>(items: Store<T>) -> i64 { return store_capacity(items) }
pub fn identity<T: StoreElement>(value: T) -> T { return value }
pub fn seed<T: StoreElement>(value: T, capacity: i64) -> Store<T> contract { result(create) } {
    let items: Store<T> = store_new()
    if !store_reserve(items, capacity) { return give items }
    store_push(items, value)
    return give items
}
