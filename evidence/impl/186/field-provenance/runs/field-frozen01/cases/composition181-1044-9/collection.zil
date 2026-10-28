pub fn empty<T: StoreElement>() -> Store<T> contract { result(create) } { return store_new() }
pub fn reserve<T: StoreElement>(items: Store<T>, capacity: i64) -> bool { return store_reserve(items, capacity) }
pub fn push<T: StoreElement>(items: Store<T>, item: T) -> T!void { return store_push(items, item) }
pub fn pop<T: StoreElement>(items: Store<T>) -> void!T { return store_pop(items) }
pub fn len<T: StoreElement>(items: Store<T>) -> i64 { return store_len(items) }
pub fn relay<T: StoreElement>(give items: Store<T>) -> Store<T> contract { result(give) } { return give items }
