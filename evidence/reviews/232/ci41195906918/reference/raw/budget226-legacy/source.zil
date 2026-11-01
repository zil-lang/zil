package Item { manages bytes: ByteBuffer; value: i32 }

fn make() -> impl Fn() -> i32 {
    let items: Store<Item> = store_new()
    store_reserve(items, 1)
    store_push(items, Item { bytes: bytes_new(), value: 42 })
    return move () => { return i32(store_len(items)) }
}

fn main() -> i32 {
    let callback = make()
    return callback() - 1
}
