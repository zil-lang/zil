package Item { manages bytes: ByteBuffer; value: i32 }
package Bag { manages items: Store<Item>; manages bytes: ByteBuffer }

fn make() -> Bag {
    let items: Store<Item> = store_new()
    store_reserve(items, 1)
    store_push(items, Item { bytes: bytes_new(), value: 42 })
    let bag = Bag { items: give items, bytes: bytes_new() }
    return bag
}

fn main() -> i32 {
    let bag = make()
    return i32(store_len(bag.items)) - 1
}
