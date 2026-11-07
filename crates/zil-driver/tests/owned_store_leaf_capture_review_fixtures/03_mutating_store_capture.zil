package Item { manages bytes: ByteBuffer; value: i32 }

fn make() -> impl FnMut() -> i32 {
    let items: Store<Item> = store_new()
    return move () => {
        if !store_reserve(items, store_len(items) + 1) { return -1 }
        store_push(items, Item { bytes: bytes_new(), value: 42 })
        return i32(store_len(items))
    }
}

fn main() -> i32 {
    var callback = make()
    if callback() != 1 { return 1 }
    return callback() - 2
}
