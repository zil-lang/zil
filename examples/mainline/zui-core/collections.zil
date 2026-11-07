// Ordinary cross-module generic helpers, with no special Session knowledge.
pub fn reserve<T:StoreElement>(items: Store<T>, capacity: i64) -> bool {
    return store_reserve(items, capacity)
}
pub fn push<T:StoreElement>(items: Store<T>, value: T) -> T!void {
    return store_push(items, value)
}
